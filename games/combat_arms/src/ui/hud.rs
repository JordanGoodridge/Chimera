//! Combat Arms' HUD (specs/combat_arms/hud_crosshair.md): the crosshair
//! (shapes, the spread gap, the firing kick, colours, the hit indicator),
//! the damage-taken overlay and direction indicator, the health, armour
//! and stamina block, the weapon and ammo panel, the kill feed, name
//! tags, and the pickup prompt (inventory.md 11), laid out on the
//! 1024x768 reference canvas stretched to the screen.
//!
//! Behaviour and drawing are apart: `model` is the spec as plain
//! functions; `CaHudStatePlugin` reads the world each frame into `CaHud`
//! (what is shown, where, in which colour, with which text) with no
//! window, so the scenario tests check it headless (tests/ui/hud.rs);
//! `CaHudPlugin` adds it and draws `CaHud` with the shell's UI nodes,
//! telling the shell's fallback HUD (`client::hud::GameDraws`) to leave
//! out its crosshair, health and ammo, hit marker and kill feed.
//!
//! Inputs: the viewed player (the local one, or whom it watches in first
//! person), its `core::Health`, its armour (`game::weapons::CaArmor`'s
//! points, else `weapon::Armor`), `Inventory`, `Zoomed`,
//! `Intent` (crouch held) and `game::movement::CaMovement` (moving, the
//! crouch blend, stamina); the held weapon's spread fraction r
//! (`game::weapons::CaSpread::fraction`; without it, r read back from
//! `Hitscan::spread`, the cone in perturb units / 1000), its data's
//! `combat_arms:spread` limits (`cone`, `zoomed_cone`, `burst_cone`) and
//! `combat_arms:hud` fields (`game::hud::CaHudData`), `Magazine`,
//! `AltModes`/`Burst`, `game::inventory::CaItem`; `WeaponEvent::Fired`
//! (the kick), `core::Damage` (the hit indicator, damage taken, the
//! name-tag flash), `core::Died` (the kill feed), `RoundRestarts` (a new
//! round clears the hit indicator); the aim ray (`client::aim`). On a
//! network client (`NetRole::Client`, which deals no damage) the hits are
//! the server's confirmations (`WeaponEventKind::Hit` of the local
//! player), the damage taken its `core::DamageTaken`, and the pickup
//! prompt the server's (`game::net::CaPickup`).
//!
//! Known shortcuts (docs/tech-debt.md; the spec's most literal reading
//! where it is unclear):
//! - The HUD runs once per rendered frame on two clocks (Open question
//!   13): real time in whole ms (`Time<Real>`) for the hit indicator, the
//!   damage overlay and the kill feed, the game's time in seconds
//!   (`SimClock`) for the kick; Fired events are timed at the frame that
//!   reads them.
//! - "Enemy" for the hit indicator and name tags: a team id that differs
//!   from the local player's (the spec's, Open question 15); with no
//!   teams (the skeleton) everyone else, as if each player had an id of
//!   their own (which ids free-for-all modes give: a live check).
//!   Damage counts when a `core::Damage` from the local player carries
//!   health (`amount` > 0), as the spec's "server reports damage".
//! - A round's end: the next round's start (`RoundRestarts`).
//! - Spec yaw grows turning right: ours is `-Intent::yaw` (Q4).
//! - Name tag opacity (255 seen, 125 blocked) multiplies the name
//!   colour's own alpha (200); no models, so no name socket: the tag is
//!   anchored at the character's position (the hull centre, the spec's
//!   fallback); no distance limit, hidden only behind the camera (Q10).
//! - Text (Q2 and Q3, settled by ui_system.md): placed by `ui::layout`
//!   (its T2-T6): fonts of the font list (`data/ui/fonts.toml`) scaled
//!   ⌊H_f y_r⌋ tall and ⌊a_f x_r⌋ a glyph; aligned texts in their box,
//!   their own position ignored on the aligned axis (the weapon name
//!   right-aligned in its 250 x 10 box and centred on its y, the values
//!   centred, labels and ammo at their positions); outlines and shadows
//!   1 px at every size; the kill feed's names advance by their reference
//!   width ⌊w_px / x_r⌋ (`feed_width`). Glyph widths are our monospaced
//!   estimate (ui_system.md's Open question 3), drawn with the install's
//!   faces or stand-ins (`ui::draw`).
//! - The weapon panel's class is the inventory class (the data's `slot`,
//!   Q16); the fire-mode icon's `RapidFireCount` comes from the trigger
//!   (`rapid_fire_count`); continuous fire isn't done, so never on.
//! - The held weapon's attachments (`game::attachments::CaMods`): the
//!   sight's range readout (A9; the local player's view ray, not a
//!   spectated player's) and the gauge sight's images (attachments.md;
//!   placeholders: their numbers), the crosshair's attachment terms (A3).
//! - Not driven by anything yet (the model has them): crosshair items,
//!   the toggle, video and AI freeze, infection, dot-sight mode (A6, A8:
//!   no sight has `Type` 2), revealing items, the score panel and round
//!   timer (no modes yet: `model::score_panel`, `round_timer`), badges
//!   but the headshot, the heartbeat's sound (counted in
//!   `CaHud::heartbeats`, no sound file).
//! - Placeholders for the game's textures: the silhouette (a grey box
//!   standing or crouched), the fire-mode icon (a word), kill-feed weapon
//!   icons and badges (words), the hit overlay (a red frame), the
//!   direction indicator (a red bar along the square's top edge), the
//!   dot-sight image (a red outline), the panels' backgrounds (dark
//!   boxes).

pub mod model;

use std::collections::HashMap;

use avian3d::prelude::*;
use bevy::{prelude::*, ui::UiTransform, window::PrimaryWindow};
use serde::Deserialize;

use crate::{
    client::{FirstPersonCamera, aim, fonts::UiFonts, hud::GameDraws, spectate::SpecView},
    core::{
        Damage, DamageKind, DamageTaken, Dead, Died, Health, Hitgroup, Intent, LocalPlayer, MovementState, NetRole,
        RoundRestarts, SimClock, Team,
    },
    game::{
        attachments::{CaMods, gauge_image},
        hud::CaHudData,
        inventory::{self, CaItem, CaSelection, Dropped},
        movement::{CaCharacter, CaMovement, movement_data},
        net::CaPickup,
        weapons::{CaArmor, CaSpread},
    },
    map::interp::RenderedView,
    weapon::{
        AltModes, Armor, Burst, Hitscan, Inventory, Magazine, SpreadShape, Trigger, Weapon, WeaponEvent, WeaponEventKind,
        Zoomed, grenade::Throwable,
    },
};
use model::*;

use super::{
    CaUi,
    draw::{self, CaFaces},
    layout::{self, Effect, FontList, REFERENCE, TextLook, text},
};
use crate::client::canvas::{HAlign, Scale, VAlign};

/// The kill feed's names' font (D5).
pub const FEED_FONT: &str = "bold 12";

/// Metres per world unit (the aim ray's reach).
const UNIT: f32 = inventory::UNIT;
/// The model's InterfaceMaxHealth (standard soldiers).
const INTERFACE_MAX_HEALTH: f32 = 100.0;

// ---------------------------------------------------------------------------
// State

/// The screen the HUD lays out on, px: the window's (UI) size in the
/// game; tests set it.
#[derive(Resource, Clone, Copy, Debug, Default, PartialEq)]
pub struct HudScreen(pub Screen);

/// What the spec has that nothing in the game drives yet: crosshair
/// items, the crosshair toggle, infection. Tests set them.
#[derive(Resource, Clone, Debug, PartialEq)]
pub struct HudOptions {
    /// The crosshair toggled on (A7; starts on).
    pub crosshair_on: bool,
    /// An equipped crosshair item's shape and colour (A6), and whether
    /// the room allows it.
    pub item_shape: Option<u8>,
    pub item_colour: Option<Rgba>,
    pub item_allowed: bool,
    pub infected: bool,
}

impl Default for HudOptions {
    fn default() -> Self {
        Self {
            crosshair_on: true,
            item_shape: None,
            item_colour: None,
            item_allowed: true,
            infected: false,
        }
    }
}

/// The crosshair this frame.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct CrosshairView {
    pub draws: Draws,
    pub shape: u8,
    /// The limits and r the gap came from.
    pub limits: (i32, i32),
    pub r: f32,
    pub measures: Option<Measures>,
    /// What is drawn (empty when `draws.lines` is off).
    pub elements: Vec<Element>,
    pub colour: Rgba,
    pub dot_sight: Option<PxRect>,
    /// The sight's range readout (A9): text and colour.
    pub range: Option<(String, Rgba)>,
    /// The gauge sight's images (attachments.md, "Crosshair, HUD and
    /// overlay"): health on the left, clip on the right, each 0..100 in
    /// tens.
    pub gauge: Option<(i32, i32)>,
}

/// The damage-taken pieces this frame.
#[derive(Clone, Debug, PartialEq)]
pub struct DamageView {
    pub alpha: u8,
    pub overlay: PxRect,
    pub corners: [Vec2; 4],
}

/// The health, armour and stamina block.
#[derive(Clone, Debug, PartialEq)]
pub struct StatusView {
    pub hp: String,
    pub ap: String,
    pub low: bool,
    pub colours: HealthColours,
    pub crouched: bool,
    /// The stamina bar's width, ref px (0: hidden).
    pub stamina_bar: f32,
}

/// A name tag this frame.
#[derive(Clone, Debug, PartialEq)]
pub struct TagView {
    pub entity: Entity,
    pub name: String,
    pub tag: Tag,
    /// Where it floats (world, metres).
    pub anchor: Vec3,
    /// Health as a fraction, for the HP bar.
    pub health: f32,
}

/// A kill-feed line: its top (ref) and its elements.
#[derive(Clone, Debug, PartialEq)]
pub struct FeedLineView {
    pub y: f32,
    pub kill: Kill,
    pub items: Vec<FeedItem>,
}

/// What Combat Arms' HUD shows this frame.
#[derive(Resource, Clone, Debug, Default, PartialEq)]
pub struct CaHud {
    pub screen: Screen,
    pub crosshair: CrosshairView,
    pub damage: Option<DamageView>,
    pub status: Option<StatusView>,
    pub weapon: Option<WeaponPanel>,
    pub feed: Vec<FeedLineView>,
    pub tags: Vec<TagView>,
    /// The local player's aim target (A11).
    pub aim_target: Option<Entity>,
    pub pickup: Option<String>,
    /// Heartbeats played so far (D2).
    pub heartbeats: u32,
}

/// The HUD's memory between frames.
#[derive(Resource, Default, Debug)]
pub struct HudMemory {
    pub hit: HitIndicator,
    pub damage: Option<DamageFeedback>,
    pub heartbeat: Heartbeat,
    pub feed: KillFeed,
    /// Weapon: when it last fired, and last fired its alternate mode (s).
    pub fired: HashMap<Entity, (Option<f64>, Option<f64>)>,
    /// Who the local player last hit, and when (s): the name-tag flash.
    pub flashes: HashMap<Entity, f64>,
    pub colour: Option<Rgba>,
    pub rounds: Option<u32>,
}

/// `CaHudStatePlugin`'s systems.
#[derive(SystemSet, Clone, Debug, PartialEq, Eq, Hash)]
pub struct HudUpdate;

/// The HUD's behaviour without a window (the scenario tests').
pub struct CaHudStatePlugin;

impl Plugin for CaHudStatePlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<HudScreen>()
            .init_resource::<CaUi>()
            .init_resource::<HudOptions>()
            .init_resource::<HudMemory>()
            .init_resource::<CaHud>()
            .add_message::<WeaponEvent>()
            .add_message::<Damage>()
            .add_message::<DamageTaken>()
            .add_message::<Died>()
            .add_systems(Update, (track, update).chain().in_set(HudUpdate));
    }
}

/// The player the HUD is about: whom the local player watches in first
/// person, else the local player.
fn viewed(spec: Option<&SpecView>, local: Option<Entity>) -> Option<Entity> {
    spec.and_then(|s| s.in_eye).or(local)
}

fn enemies(a: Option<&Team>, b: Option<&Team>) -> bool {
    !matches!((a, b), (Some(a), Some(b)) if a == b)
}

/// The horizontal forward and right of a yaw (x, z).
fn flat_axes(yaw: f32) -> (Vec2, Vec2) {
    let turn = Quat::from_rotation_y(yaw);
    let f = turn * Vec3::NEG_Z;
    let r = turn * Vec3::X;
    (Vec2::new(f.x, f.z), Vec2::new(r.x, r.z))
}

fn name_of(names: &Query<Option<&Name>>, e: Entity) -> String {
    names.get(e).ok().flatten().map_or_else(|| "Player".to_string(), |n| n.to_string())
}

/// The HUD's two clocks (Units and conventions, "Time"): the OS
/// millisecond clock (real time, whole ms) for the hit indicator, the
/// damage overlay and the kill feed, and the game's time in seconds (the
/// simulation's, else the frame's) for the crosshair kick.
fn clocks(real: &Time<Real>, time: &Time, sim: Option<&SimClock>) -> (f64, f64) {
    let ms = real.elapsed().as_millis() as f64;
    (ms, sim.map_or_else(|| time.elapsed_secs_f64(), |c| c.now))
}

/// The events of this frame: shots (the kick), damage dealt and taken,
/// kills, a new round.
#[allow(clippy::too_many_arguments)]
fn track(
    (time, real, sim): (Res<Time>, Res<Time<Real>>, Option<Res<SimClock>>),
    mut memory: ResMut<HudMemory>,
    mut weapons: MessageReader<WeaponEvent>,
    (mut damage, mut taken): (MessageReader<Damage>, MessageReader<DamageTaken>),
    mut died: MessageReader<Died>,
    local: Option<Single<(Entity, &Intent, Option<&Team>), With<LocalPlayer>>>,
    teams: Query<(Option<&Team>, Has<Health>)>,
    names: Query<Option<&Name>>,
    held: Query<&Inventory>,
    ids: Query<&Weapon>,
    (rounds, role): (Option<Res<RoundRestarts>>, Option<Res<NetRole>>),
) {
    let (ms, now) = clocks(&real, &time, sim.as_deref());
    let memory = &mut *memory;
    // A network client deals no damage: what the server says instead.
    let client = role.is_some_and(|r| *r == NetRole::Client);
    let mut dealt: Vec<Damage> = Vec::new();
    for e in weapons.read().filter(|e| e.shown()) {
        match e.kind {
            WeaponEventKind::Fired { mode, .. } => {
                let entry = memory.fired.entry(e.weapon).or_default();
                entry.0 = Some(now);
                if mode != 0 {
                    entry.1 = Some(now);
                }
            }
            WeaponEventKind::Hit { target, amount, hitgroup } if client => dealt.push(Damage {
                target,
                attacker: Some(e.owner),
                amount,
                point: Vec3::ZERO,
                dir: Vec3::ZERO,
                hitgroup,
                kind: DamageKind::Bullet,
                weapon: None,
                force: Vec3::ZERO,
            }),
            _ => {}
        }
    }
    if client {
        damage.clear();
        dealt.extend(taken.read().map(|t| t.damage.clone()));
    } else {
        taken.clear();
        dealt.extend(damage.read().cloned());
    }
    // The damage overlay fades on every frame after its hit.
    if let Some(d) = memory.damage.as_mut()
        && !d.frame(ms)
    {
        memory.damage = None;
    }
    let (me, intent, my_team) = match local.as_deref() {
        Some(&(e, i, t)) => (Some(e), Some(i.yaw), t.copied()),
        None => (None, None, None),
    };
    for d in &dealt {
        if d.amount <= 0.0 {
            continue;
        }
        if me.is_some() && d.attacker == me && d.target != d.attacker.unwrap() {
            let Ok((theirs, character)) = teams.get(d.target) else {
                continue;
            };
            let enemy = character && enemies(my_team.as_ref(), theirs);
            memory.hit.hit(ms, enemy);
            if enemy {
                memory.flashes.insert(d.target, now);
            }
        }
        if Some(d.target) == me
            && let Some(yaw) = intent
        {
            let (f, rt) = flat_axes(yaw);
            let dir = Vec2::new(d.dir.x, d.dir.z);
            memory.damage = DamageFeedback::hit(ms, d.amount, f, rt, dir, -yaw).or(memory.damage);
        }
    }
    for d in died.read() {
        let mate = |e: Entity| Some(e) == me || !enemies(my_team.as_ref(), teams.get(e).ok().and_then(|t| t.0));
        let killer = d.attacker.filter(|a| *a != d.entity);
        let kind = d.damage.kind;
        let icon = match (killer, kind) {
            (_, DamageKind::Crush | DamageKind::Fall) => KillIcon::Suicide,
            (None, DamageKind::Blast) => KillIcon::Barrel,
            (None, DamageKind::Burn) => KillIcon::Fire,
            (None, _) => KillIcon::Bleeding,
            (Some(k), _) => KillIcon::Weapon(d.damage.weapon.map(str::to_string).or_else(|| {
                let w = held.get(k).ok()?.active?;
                Some(ids.get(w).ok()?.id.to_string())
            })),
        };
        let badges = if d.damage.hitgroup == Hitgroup::Head && killer.is_some() {
            vec![Badge::Headshot]
        } else {
            Vec::new()
        };
        memory.feed.push(
            ms,
            Kill {
                killer: killer.map(|k| (name_of(&names, k), mate(k))),
                victim: (name_of(&names, d.entity), mate(d.entity)),
                icon,
                badges,
            },
        );
    }
    memory.feed.frame(ms);
    let round = rounds.map(|r| r.0);
    if memory.rounds.is_some() && memory.rounds != round {
        memory.hit.reset();
    }
    memory.rounds = round;
    memory.flashes.retain(|_, t| now - *t < TAG_FLASH_S);
}

/// The `combat_arms:spread` limits the crosshair reads (the firing part's
/// data; other fields ignored).
#[derive(Clone, Copy, Debug, Default, Deserialize)]
struct Cone {
    #[serde(default)]
    min: i32,
    #[serde(default)]
    max: i32,
}

#[derive(Clone, Copy, Debug, Default, Deserialize)]
struct SpreadLimits {
    #[serde(default)]
    cone: Cone,
    #[serde(default)]
    zoomed_cone: Option<Cone>,
    #[serde(default)]
    burst_cone: Option<Cone>,
}

/// A kill-feed name's width in ref px (Q2, ui_system.md T3): measured in
/// screen px with its font scaled to the screen, ⌊w_px / x_r⌋.
pub fn feed_width(fonts: &FontList, screen: Screen, s: &str) -> f32 {
    fonts
        .get(FEED_FONT)
        .map_or(0.0, |f| text::measure(s, f, 0, scale(screen)).reference.x as f32)
}

/// The HUD's screen scale (x_r, y_r).
pub fn scale(screen: Screen) -> Scale {
    Scale::of(Vec2::new(screen.w, screen.h), REFERENCE)
}

/// The firing part's spread hook (weapon_firing.md).
pub const SPREAD_HOOK: &str = "combat_arms:spread";

/// The crosshair's perturb data and r for a weapon whose cone is `n`
/// perturb units, from its data's spread limits (`limits`, the
/// `combat_arms:spread` table) and its HUD data, in `stance`.
pub fn spread_of(limits: Option<&toml::Value>, hud: &CaHudData, n: f32, stance: Stance) -> (Perturb, f32) {
    let Some(l) = limits.and_then(|v| v.clone().try_into::<SpreadLimits>().ok()) else {
        let n = n.round() as i32;
        let p = Perturb {
            min: n,
            max: n,
            ..default()
        };
        return (p, 0.0);
    };
    let z = l.zoomed_cone.unwrap_or_default();
    let b = l.burst_cone.unwrap_or_default();
    // The firing part's own limits in this mode (its non-zero zoomed and
    // burst values), to read r back from the cone.
    let pick = |v: i32, base: i32| if v != 0 { v } else { base };
    let (lo, hi) = if stance.zoomed && l.zoomed_cone.is_some() {
        (pick(z.min, l.cone.min), pick(z.max, l.cone.max))
    } else if stance.burst && l.burst_cone.is_some() {
        (pick(b.min, l.cone.min), pick(b.max, l.cone.max))
    } else {
        (l.cone.min, l.cone.max)
    };
    let r = if hi > lo { ((n - lo as f32) / (hi - lo) as f32).max(0.0) } else { 0.0 };
    let p = Perturb {
        min: l.cone.min,
        max: l.cone.max,
        zoomed_min: z.min,
        zoomed_max: z.max,
        burst_min: b.min,
        burst_max: b.max,
        duck: hud.duck_perturb,
        move_duck: hud.move_duck_perturb,
    };
    (p, r)
}

/// A weapon's `RapidFireCount` from its trigger: 0 fully automatic, 1
/// semi-automatic, n stopping after n (none: 1).
pub fn rapid_fire_count(trigger: Option<&Trigger>) -> u32 {
    match trigger {
        Some(t) if t.per_press >= 2 => t.per_press,
        Some(t) if t.automatic => 0,
        _ => 1,
    }
}

type Viewed<'a> = (
    &'a Health,
    Option<&'a Inventory>,
    Option<&'a Zoomed>,
    Option<&'a Intent>,
    Option<&'a CaMovement>,
    Option<&'a CaCharacter>,
    Option<&'a Armor>,
    Has<Dead>,
    Option<&'a CaArmor>,
);

type HeldWeapon<'a> = (
    &'a Weapon,
    Option<&'a Hitscan>,
    Option<&'a CaHudData>,
    Option<&'a AltModes>,
    Option<&'a Burst>,
    Option<&'a Magazine>,
    Option<&'a CaItem>,
    Option<&'a CaSpread>,
    (Option<&'a Trigger>, Option<&'a Throwable>, Option<&'a CaMods>),
);

type Character<'a> = (
    Entity,
    &'a Transform,
    &'a Health,
    Option<&'a Team>,
    Option<&'a Name>,
    Has<Dead>,
    Option<&'a ColliderAabb>,
);

#[derive(bevy::ecs::system::SystemParam)]
struct HudInputs<'w, 's> {
    local: Option<
        Single<
            'w,
            's,
            (
                Entity,
                &'static Transform,
                &'static Intent,
                &'static MovementState,
                Option<&'static RenderedView>,
                Option<&'static Team>,
                Option<&'static CaSelection>,
                Option<&'static CaPickup>,
            ),
            With<LocalPlayer>,
        >,
    >,
    spec: Option<Res<'w, SpecView>>,
    viewed: Query<'w, 's, Viewed<'static>>,
    weapons: Query<'w, 's, HeldWeapon<'static>>,
    characters: Query<'w, 's, Character<'static>, With<Intent>>,
    parents: Query<'w, 's, &'static ChildOf>,
    dropped: Query<'w, 's, &'static Dropped>,
    ids: Query<'w, 's, &'static Weapon>,
    spatial: SpatialQuery<'w, 's>,
}

/// This frame's `CaHud` from the world and the memory.
fn update(
    (time, real, sim): (Res<Time>, Res<Time<Real>>, Option<Res<SimClock>>),
    screen: Res<HudScreen>,
    options: Res<HudOptions>,
    ui: Res<CaUi>,
    mut memory: ResMut<HudMemory>,
    world: HudInputs,
    mut hud: ResMut<CaHud>,
) {
    let (ms, now) = clocks(&real, &time, sim.as_deref());
    let screen = screen.0;
    let memory = &mut *memory;
    let local = world.local.as_deref().copied();
    let in_eye = world.spec.as_deref().and_then(|s| s.in_eye);
    let who = viewed(world.spec.as_deref(), local.map(|l| l.0));
    let player = who.and_then(|e| world.viewed.get(e).ok());
    let hit = memory.hit.frame(ms);
    let mut next = CaHud {
        screen,
        heartbeats: hud.heartbeats,
        ..default()
    };

    // The crosshair (A).
    let active = player.and_then(|p| p.1).and_then(|i| i.active);
    let weapon = active.and_then(|w| world.weapons.get(w).ok().map(|x| (w, x)));
    let dead = player.is_none_or(|p| p.7 || p.0.current <= 0.0);
    let scope = player.and_then(|p| p.2).is_some_and(|z| z.scope);
    let default_hud = CaHudData::default();
    let data = weapon.and_then(|(_, w)| w.2).unwrap_or(&default_hud);
    // The held weapon's attachments (its sight's readout and gauge).
    let mods = weapon.and_then(|(_, w)| w.8.2);
    let visibility = model::Visibility {
        ok: options.crosshair_on,
        scope_overlay: scope,
        death_cam: dead && in_eye.is_none(),
        weapon: weapon.is_some(),
        disabled: data.disable_crosshair,
        dot_sight: data.dot_sight.is_some(),
        range_readout: mods.is_some_and(|m| m.range_readout().is_some()),
    };
    let draws = model::draws(visibility);
    let colour = choose_colour(
        ColourInputs {
            hit,
            dot_sight: None,
            item: options.item_colour,
            weapon: weapon.map(|_| data.crosshair_color),
        },
        memory.colour.unwrap_or(DEFAULT_CROSSHAIR),
    );
    memory.colour = Some(colour);
    next.crosshair = CrosshairView {
        draws,
        colour,
        dot_sight: (draws.dot_sight > 0).then(|| dot_sight_rect(screen)),
        ..default()
    };
    if let (Some((w, (id, scan, _, modes, burst, _, _, spread, _))), Some(p)) = (weapon, player) {
        let n = scan.map_or(0.0, |s| match s.spread {
            SpreadShape::Template(s) => s,
            SpreadShape::Disc { inaccuracy, spread } => inaccuracy + spread,
        }) * 1000.0;
        let bursting = matches!((modes, burst), (Some(m), Some(b)) if b.mode == m.current && b.count > 0);
        let stance = Stance {
            zoomed: p.2.is_some_and(|z| !z.scope),
            burst: bursting,
            crouch: p.3.is_some_and(|i| i.crouch),
            moving: p.4.is_some_and(|m| m.moving),
        };
        let limits = crate::weapon::data::lookup(id.id).and_then(|d| d.hooks.get(SPREAD_HOOK));
        let (perturb, r) = spread_of(limits, data, n, stance);
        // The firing part's own fraction when it runs on this weapon.
        let r = spread.map_or(r, CaSpread::fraction);
        let perturb_mods = PerturbMods {
            attachments: mods.map(CaMods::crosshair_terms).unwrap_or_default(),
            ..default()
        };
        let (min, max) = perturb_limits(&perturb, &perturb_mods, stance);
        let (fired, alt) = memory.fired.get(&w).copied().unwrap_or_default();
        let since = |t: Option<f64>| t.map(|t| (now - t) as f32);
        let shape = choose_shape(data.crosshair, options.item_shape, options.item_allowed);
        let (m, elements) = crosshair_elements(
            &CrosshairInputs {
                weapon_shape: data.crosshair,
                shape,
                min,
                max,
                r,
                since_fire: since(fired),
                since_alt: since(alt),
                offset: data.crosshair_offset,
                wide: data.wide_count,
            },
            screen,
        );
        let c = &mut next.crosshair;
        c.shape = shape;
        c.limits = (min, max);
        c.r = r;
        c.measures = Some(m);
        if draws.lines {
            c.elements = elements;
        }
    }

    // The sight (attachments.md, "Crosshair, HUD and overlay"): the range
    // readout to what the local player's view ray hits within the sight's
    // reach (A9), the gauge of health and clip.
    if let (Some(m), Some((_, w))) = (mods, weapon) {
        if draws.range_readout
            && let Some(reach) = m.range_readout()
            && let Some((me, at, intent, state, view, ..)) = local
            && who == Some(me)
        {
            let ray = aim::view_ray(at, intent, state, view);
            let filter = SpatialQueryFilter::from_excluded_entities([me]).with_mask(crate::core::SOLID_LAYERS);
            let hit = world
                .spatial
                .cast_ray(ray.eye, ray.dir, reach as f32 * UNIT, true, &filter)
                .map(|h| h.distance / UNIT);
            let range = w.1.map_or(0.0, |s| (s.range / UNIT).round());
            next.crosshair.range = Some(range_readout(hit, range, m.damage().range_per));
        }
        if m.gauge()
            && let Some(p) = player
        {
            // The integer percentage (health is a fraction here: a hair of
            // slack so 0.95 is 95, not 94.99).
            let percent = |v: f32, max: f32| if max > 0.0 { (100.0 * v / max + 1e-4).floor() as i32 } else { 0 };
            let hp = percent(p.0.current, p.0.max);
            let clip = w.5.map_or(0, |mag| percent(mag.clip as f32, mag.size as f32));
            next.crosshair.gauge = Some((gauge_image(hp), gauge_image(clip)));
        }
    }

    // Damage taken (C): drawn whenever the crosshair item draws.
    if draws.damage
        && let Some(d) = memory.damage
    {
        let yaw = local.map_or(0.0, |l| -l.2.yaw);
        next.damage = Some(DamageView {
            alpha: d.alpha,
            overlay: hit_overlay_rect(screen),
            corners: d.corners(yaw, screen),
        });
    }

    // Health, armour, stamina (D2, D3): the local player's, alive and not
    // watching anyone.
    let status_shown = who.is_some() && who == local.map(|l| l.0) && !dead;
    if status_shown && let Some(p) = player {
        let (h, max) = (p.0.current * 100.0, p.0.max * 100.0);
        let low = low_health(h, max, options.infected);
        if memory.heartbeat.frame(now, low) {
            next.heartbeats += 1;
        }
        let character = p.5.unwrap_or(&movement_data().character);
        next.status = Some(StatusView {
            hp: hp_shown(h, max, INTERFACE_MAX_HEALTH).to_string(),
            ap: ap_shown(p.8.map_or_else(|| p.6.map_or(0.0, |a| a.amount * 100.0), |a| a.points)).to_string(),
            low,
            colours: health_colours(low),
            crouched: crouch_image(p.4.map_or(0.0, |m| m.crouch_rate)),
            stamina_bar: p.4.map_or(0.0, |m| stamina_bar(m.stamina, character.stamina_max())),
        });
    }

    // The weapon panel (D4).
    if let Some((_, (id, _, hud_data, modes, burst, mag, item, _, (trigger, throw, _)))) = weapon {
        let data = crate::weapon::data::lookup(id.id);
        let bursting = match (modes, burst) {
            (Some(m), Some(b)) if b.mode == m.current && b.count > 0 => Some(b.count),
            _ => None,
        };
        // A thrown weapon's count is its clip, nothing in reserve.
        let (clip, total) = match (mag, throw) {
            (Some(m), _) => (m.clip, m.clip + m.reserve),
            (None, Some(t)) => (t.count, t.count),
            _ => (0, 0),
        };
        let inputs = PanelInputs {
            name: data
                .and_then(|d| d.hud.name.clone())
                .unwrap_or_else(|| id.id.rsplit(':').next().unwrap_or(id.id).to_uppercase()),
            class: inventory::class_of(id.slot),
            bomb: hud_data.is_some_and(|h| h.bomb),
            infinite: item.is_some_and(|i| i.infinite) || (mag.is_none() && throw.is_none()),
            clip,
            total,
            burst: bursting,
            rapid_fire_count: rapid_fire_count(trigger),
            // Continuous fire isn't done (weapon_firing.md): never on.
            continuous: false,
        };
        next.weapon = model::weapon_panel(&inputs, status_shown && !options.infected);
    }

    // The kill feed (D5): shown even while dead.
    next.feed = memory
        .feed
        .lines
        .iter()
        .enumerate()
        .map(|(i, (_, k))| FeedLineView {
            y: KillFeed::line_y(i),
            kill: k.clone(),
            items: feed_line(k, |s| feed_width(&ui.fonts, screen, s)),
        })
        .collect();

    // Name tags and the aim target (A11).
    if let Some((me, at, intent, state, view, my_team, selection, served)) = local {
        let ray = aim::view_ray(at, intent, state, view);
        let local_alive = world.viewed.get(me).is_ok_and(|p| !p.7 && p.0.current > 0.0);
        let target = if local_alive {
            aim::character_under(
                &world.spatial,
                ray,
                AIM_RANGE * UNIT,
                me,
                |e| world.characters.contains(e),
                &world.parents,
            )
            .map(|(e, _)| e)
        } else {
            None
        };
        next.aim_target = target;
        for (e, t, health, team, name, their_dead, _) in &world.characters {
            if e == me {
                continue;
            }
            // The model's name socket; without one (no models yet) the
            // character's position, the hull centre (Open question 10).
            let anchor = t.translation;
            let inputs = TagInputs {
                local_alive,
                teammate: !enemies(my_team, team),
                alive: !their_dead && health.current > 0.0,
                aimed: target == Some(e),
                revealed: false,
                flashing: memory.flashes.contains_key(&e),
                seen: aim::clear_line(&world.spatial, ray.eye, anchor, &[me, e]),
                chased: in_eye == Some(e),
                state_viewer: false,
            };
            if let Some(tag) = name_tag(inputs) {
                next.tags.push(TagView {
                    entity: e,
                    name: name.map_or_else(|| "Player".to_string(), |n| n.to_string()),
                    tag,
                    anchor,
                    health: if health.max > 0.0 { health.current / health.max } else { 0.0 },
                });
            }
        }
        // The pickup prompt (inventory.md 11): the item the view is on.
        if status_shown {
            // Here, or (a network client) the server's.
            let here = selection
                .and_then(|s| s.pickup_target)
                .and_then(|i| world.dropped.get(i).ok())
                .and_then(|d| world.ids.get(d.weapon).ok())
                .map(|w| w.id.to_string());
            next.pickup = here.or_else(|| served.and_then(|p| p.0.clone())).map(|id| {
                let name = crate::weapon::data::lookup(&id)
                    .and_then(|d| d.hud.name.clone())
                    .unwrap_or_else(|| id.rsplit(':').next().unwrap_or(&id).to_uppercase());
                format!("Press [E] to pick up {name}")
            });
        }
    }
    hud.set_if_neq(next);
}

// ---------------------------------------------------------------------------
// Drawing

/// Combat Arms' HUD, drawn: its behaviour (`CaHudStatePlugin`) and the
/// shell's UI nodes for it; the shell's fallback HUD leaves out what this
/// draws.
pub struct CaHudPlugin;

impl Plugin for CaHudPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(CaHudStatePlugin)
            .insert_resource(GameDraws {
                crosshair: true,
                status: true,
                hit_marker: true,
                killfeed: true,
            })
            .add_systems(Startup, (spawn, draw::load_faces))
            .add_systems(Update, screen_size.before(HudUpdate))
            .add_systems(
                Update,
                (draw_crosshair, draw_damage, draw_status, draw_weapon, draw_feed, draw_tags).after(HudUpdate),
            );
    }
}

fn colour(c: Rgba) -> Color {
    Color::srgba_u8(c[0], c[1], c[2], c[3])
}

/// The window's size as the UI lays out (logical px).
fn screen_size(window: Query<&Window, With<PrimaryWindow>>, mut screen: ResMut<HudScreen>) {
    if let Ok(w) = window.single() {
        let s = Screen::new(w.resolution.width(), w.resolution.height());
        if s.w > 0.0 && s.h > 0.0 && screen.0 != s {
            screen.0 = s;
        }
    }
}

#[derive(Component)]
struct CrosshairRoot;
#[derive(Component)]
struct DamageRoot;
#[derive(Component)]
struct StatusRoot;
#[derive(Component)]
struct WeaponRoot;
#[derive(Component)]
struct FeedRoot;
#[derive(Component)]
struct TagRoot;

fn layer(z: i32) -> impl Bundle {
    (
        Node {
            position_type: PositionType::Absolute,
            left: px(0.0),
            top: px(0.0),
            width: percent(100.0),
            height: percent(100.0),
            ..default()
        },
        GlobalZIndex(z),
    )
}

fn spawn(mut commands: Commands) {
    commands.spawn((CrosshairRoot, layer(40)));
    commands.spawn((DamageRoot, layer(39)));
    commands.spawn((StatusRoot, layer(40)));
    commands.spawn((WeaponRoot, layer(40)));
    commands.spawn((FeedRoot, layer(40)));
    commands.spawn((TagRoot, layer(38)));
}

/// A rect node, px.
fn rect(r: PxRect, c: Color) -> impl Bundle {
    let s = r.size();
    (
        Node {
            position_type: PositionType::Absolute,
            left: px(r.min.x),
            top: px(r.min.y),
            width: px(s.x),
            height: px(s.y),
            ..default()
        },
        BackgroundColor(c),
    )
}

/// A straight line node from `a` to `b`, `thickness` px.
fn line(a: Vec2, b: Vec2, thickness: f32, c: Color) -> impl Bundle {
    let mid = (a + b) / 2.0;
    let d = b - a;
    let len = d.length();
    (
        Node {
            position_type: PositionType::Absolute,
            left: px(mid.x - len / 2.0),
            top: px(mid.y - thickness / 2.0),
            width: px(len),
            height: px(thickness),
            ..default()
        },
        UiTransform::from_rotation(Rot2::radians(d.y.atan2(d.x))),
        BackgroundColor(c),
    )
}

/// What HUD texts are drawn with: the UI's fonts, the shell's stand-in
/// faces and the install's, on the HUD's screen.
#[derive(Clone, Copy)]
struct Pen<'a> {
    ui: &'a CaUi,
    fonts: Option<&'a UiFonts>,
    faces: Option<&'a CaFaces>,
    screen: Screen,
}

/// A HUD text (ui_system.md T2-T6, settling hud_crosshair.md Q2 and Q3):
/// `string` in `font` scaled with both factors, in a box (`at`, `size`,
/// ref px) at `own` inside it, aligned in the box on each aligned axis
/// (its own position ignored there), drawn with its effect's copies.
#[allow(clippy::too_many_arguments)]
fn hud_text(
    p: &mut ChildSpawnerCommands,
    pen: Pen,
    (at, size, own): (Vec2, Vec2, Vec2),
    string: &str,
    font: &str,
    (h, v): (HAlign, VAlign),
    effect: Effect,
    colour: Rgba,
) {
    let look = TextLook {
        font: font.to_string(),
        h,
        v,
        effect,
        colour,
        stroke: [0, 0, 0, 255],
        shadow: [0, 0, 0, 255],
        set: Some(string.to_string()),
        ..default()
    };
    if let Some(t) = layout::boxed_text(
        at.as_ivec2(),
        size.as_ivec2(),
        own.as_ivec2(),
        look,
        scale(pen.screen),
        &pen.ui.fonts,
    ) {
        draw::spawn_text(p, &t, pen.fonts, pen.faces);
    }
}

/// A text at a reference position (default alignment).
fn hud_text_at(p: &mut ChildSpawnerCommands, pen: Pen, at: Vec2, string: &str, font: &str, effect: Effect, colour: Rgba) {
    hud_text(
        p,
        pen,
        (Vec2::ZERO, Vec2::ZERO, at),
        string,
        font,
        (HAlign::Default, VAlign::Default),
        effect,
        colour,
    );
}

/// Redraw a layer's children when what it shows changed.
fn rebuild<T: PartialEq + Clone + Send + Sync + 'static>(
    last: &mut Option<T>,
    now: T,
    root: Entity,
    commands: &mut Commands,
) -> bool {
    if last.as_ref() == Some(&now) {
        return false;
    }
    *last = Some(now);
    commands.entity(root).despawn_related::<Children>();
    true
}

fn draw_crosshair(
    hud: Res<CaHud>,
    params: PenParams,
    root: Single<Entity, With<CrosshairRoot>>,
    mut last: Local<Option<(CrosshairView, Screen)>>,
    mut commands: Commands,
) {
    let c = &hud.crosshair;
    if !rebuild(&mut last, (c.clone(), hud.screen), *root, &mut commands) {
        return;
    }
    let col = colour(c.colour);
    let pen = pen(&params, hud.screen);
    commands.entity(*root).with_children(|p| {
        // The range readout, right-aligned at `RANGE_AT` (A9).
        if let Some((t, rgba)) = &c.range {
            let size = Vec2::new(120.0, 16.0);
            let boxed = (RANGE_AT - Vec2::new(size.x, 0.0), size, Vec2::ZERO);
            let align = (HAlign::Right, VAlign::Centre);
            hud_text(p, pen, boxed, t, looks::PROMPT, align, Effect::Shadow, *rgba);
        }
        // PLACEHOLDER for the gauge sight's images: their numbers beside
        // the centre, health left, clip right.
        if let Some((hp, clip)) = c.gauge {
            let size = Vec2::new(60.0, 16.0);
            let white = [255, 255, 255, 220];
            let left = (Vec2::new(512.0 - 90.0 - size.x, 376.0), size, Vec2::ZERO);
            let right = (Vec2::new(512.0 + 90.0, 376.0), size, Vec2::ZERO);
            let (l, r) = ((HAlign::Right, VAlign::Centre), (HAlign::Left, VAlign::Centre));
            hud_text(p, pen, left, &format!("HP {hp}"), looks::ICON_WORD, l, Effect::Shadow, white);
            hud_text(p, pen, right, &format!("{clip} AMMO"), looks::ICON_WORD, r, Effect::Shadow, white);
        }
        for _ in 0..c.draws.dot_sight {
            // PLACEHOLDER for the weapon's reticle image: a red outline.
            if let Some(r) = c.dot_sight {
                p.spawn((rect(r, Color::NONE), Outline::new(px(1.0), px(0.0), Color::srgba(1.0, 0.1, 0.1, 0.5))));
            }
        }
        for e in &c.elements {
            match e {
                Element::Rect(r) => {
                    p.spawn(rect(*r, col));
                }
                Element::Line(a, b) => {
                    p.spawn(line(*a, *b, 1.0, col));
                }
                Element::Circle { centre, radius } => {
                    let v = circle_vertices(*centre, *radius);
                    for k in 0..v.len() {
                        p.spawn(line(v[k], v[(k + 1) % v.len()], 1.0, col));
                    }
                }
            }
        }
    });
}

fn draw_damage(
    hud: Res<CaHud>,
    root: Single<Entity, With<DamageRoot>>,
    mut last: Local<Option<Option<DamageView>>>,
    mut commands: Commands,
) {
    if !rebuild(&mut last, hud.damage.clone(), *root, &mut commands) {
        return;
    }
    let Some(d) = &hud.damage else { return };
    let a = d.alpha as f32 / 255.0;
    commands.entity(*root).with_children(|p| {
        // PLACEHOLDER for the red edge vignette: a red frame over the
        // overlay's rect (1024 ref tall: its bottom is off the screen).
        let edge = 0.06 * hud.screen.h;
        let s = d.overlay.size();
        p.spawn((
            Node {
                position_type: PositionType::Absolute,
                left: px(d.overlay.min.x),
                top: px(d.overlay.min.y),
                width: px(s.x),
                height: px(s.y),
                border: UiRect::all(px(edge)),
                ..default()
            },
            BorderColor::all(Color::srgba(0.8, 0.0, 0.0, 0.45 * a)),
        ));
        // PLACEHOLDER for the arc: a bar along the square's top edge
        // (P0 to P1, the side facing the source), a little inside.
        let [p0, p1, ..] = d.corners;
        let centre = hud.screen.at(Vec2::new(512.0, 384.0));
        let (a0, a1) = (p0.lerp(centre, 0.12), p1.lerp(centre, 0.12));
        let (q0, q1) = (a0.lerp(a1, 0.25), a0.lerp(a1, 0.75));
        p.spawn(line(q0, q1, 6.0 * hud.screen.yr(), Color::srgba(0.85, 0.05, 0.05, 0.9 * a)));
    });
}

/// The HUD texts' fonts and looks (hud_crosshair.md D2, D4, D5).
mod looks {
    pub const LABEL: &str = "bold 16";
    pub const VALUE: &str = "number 25";
    pub const NAME: &str = "bold 16";
    pub const CLIP: &str = "number 26";
    pub const SLASH: &str = "bold 25";
    pub const RESERVE: &str = "number 25";
    /// Ours: placeholder words for icons, and the pickup prompt.
    pub const ICON_WORD: &str = "bold 11";
    pub const PROMPT: &str = "bold 14";
}

type PenParams<'a> = (Res<'a, CaUi>, Option<Res<'a, UiFonts>>, Option<Res<'a, CaFaces>>);

fn pen<'a>(params: &'a PenParams, screen: Screen) -> Pen<'a> {
    Pen {
        ui: &params.0,
        fonts: params.1.as_deref(),
        faces: params.2.as_deref(),
        screen,
    }
}

fn draw_status(
    hud: Res<CaHud>,
    params: PenParams,
    root: Single<Entity, With<StatusRoot>>,
    mut last: Local<Option<(Option<StatusView>, Option<String>, Screen)>>,
    mut commands: Commands,
) {
    if !rebuild(&mut last, (hud.status.clone(), hud.pickup.clone(), hud.screen), *root, &mut commands) {
        return;
    }
    let screen = hud.screen;
    let pen = pen(&params, screen);
    let resolve = |id: &str| pen.ui.strings.resolve(id);
    commands.entity(*root).with_children(|p| {
        if let Some(prompt) = &hud.pickup {
            // Ours: the spec gives the prompt no look or place.
            hud_text(
                p,
                pen,
                (Vec2::new(312.0, 430.0), Vec2::new(400.0, 20.0), Vec2::ZERO),
                prompt,
                looks::PROMPT,
                (HAlign::Centre, VAlign::Centre),
                Effect::Shadow,
                [255, 255, 255, 255],
            );
        }
        let Some(s) = &hud.status else { return };
        let box_of = |(at, size): (Vec2, Vec2)| screen.rect(at, size);
        // PLACEHOLDER backgrounds for the panel's textures.
        p.spawn(rect(box_of(status::PANEL), Color::srgba(0.05, 0.06, 0.07, 0.65)));
        for plate in status::PLATES {
            p.spawn(rect(box_of(plate), Color::srgba(0.0, 0.0, 0.0, 0.45)));
        }
        // PLACEHOLDER silhouette: a box, shorter when crouched, tinted red
        // at low health.
        let sil = box_of(status::SILHOUETTE);
        let tall = if s.crouched { 0.3 } else { 0.48 };
        let w = sil.size().x * 0.3;
        let h = sil.size().y * tall;
        let bottom = screen.at(Vec2::new(0.0, 766.0)).y;
        let x = sil.min.x + (sil.size().x - w) / 2.0;
        let tint = s.colours.silhouette.map_or(Color::srgba(0.75, 0.75, 0.75, 0.8), colour);
        p.spawn(rect(PxRect::new(x, x + w, bottom - h, bottom), tint));
        // Labels at their positions; values centred in their boxes.
        for (at, id, c) in [
            (status::HP_LABEL, "$hud_hp", s.colours.hp_ap_labels),
            (status::AP_LABEL, "$hud_ap", s.colours.hp_ap_labels),
            (status::SP_LABEL, "$hud_sp", s.colours.other_labels),
            (status::SET_LABEL, "$hud_set", s.colours.other_labels),
        ] {
            hud_text_at(p, pen, at, &resolve(id), looks::LABEL, Effect::Shadow, c);
        }
        for ((at, size), t) in [(status::HP_BOX, &s.hp), (status::AP_BOX, &s.ap)] {
            hud_text(
                p,
                pen,
                (at, size, Vec2::ZERO),
                t,
                looks::VALUE,
                (HAlign::Centre, VAlign::Centre),
                Effect::Shadow,
                s.colours.values,
            );
        }
        if s.stamina_bar > 0.0 {
            p.spawn(rect(
                screen.rect(status::STAMINA_BAR, Vec2::new(s.stamina_bar, status::STAMINA_BAR_HEIGHT)),
                Color::srgba(0.45, 0.75, 0.95, 0.85),
            ));
        }
    });
}

fn draw_weapon(
    hud: Res<CaHud>,
    params: PenParams,
    root: Single<Entity, With<WeaponRoot>>,
    mut last: Local<Option<(Option<WeaponPanel>, Screen)>>,
    mut commands: Commands,
) {
    if !rebuild(&mut last, (hud.weapon.clone(), hud.screen), *root, &mut commands) {
        return;
    }
    let Some(w) = &hud.weapon else { return };
    let pen = pen(&params, hud.screen);
    commands.entity(*root).with_children(|p| {
        // Right-aligned in its box, centred on its y (Q3).
        let (at, size) = panel::NAME_BOX;
        hud_text(
            p,
            pen,
            (at, size, Vec2::ZERO),
            &w.name,
            looks::NAME,
            (HAlign::Right, VAlign::Centre),
            Effect::Stroke,
            panel::NAME,
        );
        if let Some((clip, reserve)) = &w.ammo {
            hud_text_at(p, pen, panel::CLIP_AT, clip, looks::CLIP, Effect::Stroke, panel::CLIP);
            hud_text_at(p, pen, panel::SLASH_AT, "/", looks::SLASH, Effect::Stroke, panel::RESERVE);
            hud_text_at(p, pen, panel::RESERVE_AT, reserve, looks::RESERVE, Effect::Stroke, panel::RESERVE);
        }
        if let Some(icon) = w.icon {
            // PLACEHOLDER for the fire-mode icon: its name.
            let t = match icon {
                FireIcon::Burst2 => "BURST 2",
                FireIcon::Burst3 => "BURST 3",
                FireIcon::Auto => "AUTO",
                FireIcon::Single => "SEMI",
            };
            hud_text_at(p, pen, panel::ICON.0, t, looks::ICON_WORD, Effect::None, panel::RESERVE);
        }
    });
}

fn draw_feed(
    hud: Res<CaHud>,
    params: PenParams,
    root: Single<Entity, With<FeedRoot>>,
    mut last: Local<Option<(Vec<FeedLineView>, Screen)>>,
    mut commands: Commands,
) {
    if !rebuild(&mut last, (hud.feed.clone(), hud.screen), *root, &mut commands) {
        return;
    }
    let pen = pen(&params, hud.screen);
    let grey = [200, 200, 200, FEED_TEXT_ALPHA];
    commands.entity(*root).with_children(|p| {
        for l in &hud.feed {
            // Each element at the running x (`feed_line`, names advancing by
            // their measured width), 3 ref px down the line.
            for item in &l.items {
                match item {
                    FeedItem::Text { x, text, colour } => {
                        hud_text_at(p, pen, Vec2::new(*x, l.y + 3.0), text, FEED_FONT, Effect::Stroke, *colour);
                    }
                    // PLACEHOLDER icons and badges: words in their slots.
                    FeedItem::Icon { x, icon } => {
                        let word = match icon {
                            KillIcon::Weapon(Some(id)) => format!("[{}]", id.rsplit(':').next().unwrap_or(id)),
                            KillIcon::Weapon(None) => String::new(),
                            KillIcon::Suicide => "[suicide]".into(),
                            KillIcon::Barrel => "[blast]".into(),
                            KillIcon::Fire => "[fire]".into(),
                            KillIcon::Poison => "[poison]".into(),
                            KillIcon::Bleeding => "[bleed]".into(),
                            KillIcon::Npc => "[npc]".into(),
                        };
                        hud_text_at(p, pen, Vec2::new(*x, l.y + 4.0), &word, looks::ICON_WORD, Effect::Stroke, grey);
                    }
                    FeedItem::Badge { x, badge } => {
                        let word = format!("{badge:?}").chars().take(2).collect::<String>().to_uppercase();
                        hud_text_at(p, pen, Vec2::new(*x, l.y + 4.0), &word, looks::ICON_WORD, Effect::Stroke, grey);
                    }
                }
            }
        }
    });
}

fn draw_tags(
    hud: Res<CaHud>,
    fonts: Option<Res<UiFonts>>,
    root: Single<Entity, With<TagRoot>>,
    camera: Query<(&Camera, &GlobalTransform), With<FirstPersonCamera>>,
    mut commands: Commands,
) {
    commands.entity(*root).despawn_related::<Children>();
    let Some((cam, at)) = camera.iter().next() else { return };
    let pts = 12.0 * hud.screen.yr();
    commands.entity(*root).with_children(|p| {
        for t in &hud.tags {
            // No distance limit: only a tag behind the camera hides.
            let depth = (t.anchor - at.translation()).dot(at.forward().as_vec3());
            if !tag_in_front(depth) {
                continue;
            }
            let Ok(s) = cam.world_to_viewport(at, t.anchor) else {
                continue;
            };
            let opacity = t.tag.opacity as f32 / 255.0;
            let [r, g, b, a] = t.tag.name;
            let name = Color::srgba_u8(r, g, b, (a as f32 * opacity) as u8);
            let font = fonts.as_deref().map_or_else(
                || TextFont {
                    font_size: FontSize::Px(pts),
                    ..default()
                },
                |f| f.em("Tahoma", true, pts),
            );
            let (w, h) = (200.0, pts * 2.4);
            let mut tag = p.spawn(Node {
                position_type: PositionType::Absolute,
                left: px(s.x - w / 2.0),
                top: px(s.y - h),
                width: px(w),
                height: px(h),
                flex_direction: FlexDirection::Column,
                align_items: AlignItems::Center,
                justify_content: JustifyContent::End,
                ..default()
            });
            tag.with_children(|c| {
                c.spawn((Text::new(t.name.clone()), font, TextColor(name), TextLayout::no_wrap()));
                if t.tag.hp_bar {
                    c.spawn((
                        Node {
                            width: px(40.0),
                            height: px(3.0),
                            ..default()
                        },
                        BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.6 * opacity)),
                        children![(
                            Node {
                                width: percent(100.0 * t.health.clamp(0.0, 1.0)),
                                height: percent(100.0),
                                ..default()
                            },
                            BackgroundColor(Color::srgba(0.3, 0.9, 0.3, opacity)),
                        )],
                    ));
                }
            });
        }
    });
}
