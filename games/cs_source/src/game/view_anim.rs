//! What CS:S view models play (specs/cs_source/weapons.md 3.3–3.8): the
//! draw activity when a weapon is deployed, a fire activity per shot or
//! swing, the reload activity, and idle once the last one has played out;
//! the effects their animation events ask for (specs/cs_source/
//! view_models.md 6–8: muzzle flash, brass); how they are drawn (FOV,
//! handedness) and their console variables. These are CS:S's `rules` on
//! Chimera's view-model driver (`weapon::view_anim`, which drives each
//! character's `map::ViewAnimator`); `map::view_model` draws it.
//!
//! Known shortcuts (docs/tech-debt.md): View-model idle (`view_anim`): the
//! knife idles `KNIFE_IDLE` (20 s) after its draw or last attack (CS:S's holds
//! still at least 10 s; its idle turns the blade from 3.4 s in); `IdleInterval`
//! read as "least time from one idle's start to the next" (only the TMP's 5 s
//! sway shows it). Captures to confirm: the knife 15, 20, 25 and 30 s after
//! drawing it, and the TMP 1-10 s after drawing it (`setpos -295 1078 120;
//! setang 0 0 0` on de_dust2, `give weapon_tmp`). Why it's OK for now: Only the
//! knife and TMP idles move (`dump --sequences`).

use bevy::prelude::*;

use crate::{
    console::{Console, resource_cvar},
    map::{
        DriveAnimation, EffectSettings, MapFlashLight, MapMuzzleFlash, ViewModelEventKind, ViewModelSettings,
        anim::AnimEvent,
    },
    weapon::{
        WeaponEventKind,
        view_anim::{self, Pick, ViewAnimRules, ViewFrame, ViewState},
    },
};

use super::view_motion::{self, ViewMotion};

/// `viewmodel_fov` default: horizontal degrees at 4:3. CS:S's is 54 (spec
/// view_models.md, constants); ours starts at 80 by choice (a smaller,
/// less screen-filling gun), changeable in the options.
pub const VIEWMODEL_FOV: f32 = 80.0;
/// `default_fov`: the player's normal field of view, same convention.
pub const DEFAULT_FOV: f32 = 90.0;

const UNIT: f32 = 0.0254;

/// The view-model settings CS:S starts with (`cl_righthand 1`,
/// `r_drawviewmodel 1`).
pub fn settings() -> ViewModelSettings {
    ViewModelSettings {
        fov: VIEWMODEL_FOV,
        default_fov: DEFAULT_FOV,
        right_hand: 1,
        draw: 1,
    }
}

/// Model event numbers (spec view_models.md 6): a muzzle flash on the
/// attachment its option names (1-based); the named client effect that
/// ejects brass ("EjectBrass_<type> <attachment> <speed>").
const EVENT_MUZZLE_FLASHES: &[i32] = &[5001, 5011, 5021, 5031];
const EVENT_CLIENT_EFFECT_ATTACH: &str = "AE_CLIENT_EFFECT_ATTACH";

/// What a view-model animation event asks the map to show.
pub fn event_effect(e: &AnimEvent) -> Option<ViewModelEventKind> {
    if EVENT_MUZZLE_FLASHES.contains(&e.event) {
        let n: usize = e.options.trim().parse().ok()?;
        return Some(ViewModelEventKind::MuzzleFlash {
            attachment: n.checked_sub(1)?,
        });
    }
    if e.name.eq_ignore_ascii_case(EVENT_CLIENT_EFFECT_ATTACH) {
        let mut words = e.options.split_whitespace();
        let effect = words.next()?;
        let shell = effect.strip_prefix("EjectBrass_")?;
        let attachment: usize = words.next()?.parse().ok()?;
        let speed: f32 = words.next().and_then(|w| w.parse().ok()).unwrap_or(0.0);
        return Some(ViewModelEventKind::EjectBrass {
            attachment: attachment.checked_sub(1)?,
            shell: shell.to_string(),
            speed,
        });
    }
    None
}

/// Shell types (spec view_models.md 8): the name brass events use, the
/// model, and the bounce sound.
pub const SHELLS: &[(&str, &str, &str)] = &[
    ("9mm", "models/shells/shell_9mm.mdl", "Bounce.PistolShell"),
    ("57", "models/shells/shell_57.mdl", "Bounce.PistolShell"),
    ("12gauge", "models/shells/shell_12gauge.mdl", "Bounce.ShotgunShell"),
    ("556", "models/shells/shell_556.mdl", "Bounce.RifleShell"),
    ("762nato", "models/shells/shell_762nato.mdl", "Bounce.RifleShell"),
    ("338mag", "models/shells/shell_338mag.mdl", "Bounce.RifleShell"),
];

/// How CS:S's shells fly (spec view_models.md 8): full `sv_gravity` (800
/// units/s^2), half the speed kept per bounce, at rest on floors (normal up
/// part above 0.9) when falling slower than three frames of gravity,
/// angles x 0.9 per bounce, the event's speed x 1.2..2.8 forward, up to 10
/// units/s up/down and 20 left/right, spin up to 256 degrees/s, 10 s then a
/// 2 s fade, a bounce sound one hit in six, full volume at 450 units/s.
pub fn shell_physics() -> crate::map::shells::MapShellPhysics {
    crate::map::shells::MapShellPhysics {
        gravity: 800.0 * UNIT,
        damping: 0.5,
        rest_normal: 0.9,
        rest_steps: 3.0,
        angle_damping: 0.9,
        unit: UNIT,
        speed_factor: (1.2, 2.8),
        up_jitter: 10.0 * UNIT,
        right_jitter: 20.0 * UNIT,
        spin: 256f32.to_radians(),
        life: 10.0,
        fade: 2.0,
        sound_chance: 1.0 / 6.0,
        sound_full_speed: 450.0 * UNIT,
    }
}

/// CS:S's muzzle flash as we draw it (spec view_models.md 7 leaves the
/// look open; our choice, see docs/plans/active/view-models.md): CS:S's
/// own `effects/muzzleflashx` material as three view-facing sprites
/// strung forward from the muzzle, shrinking, for 0.05 s; and the SDK's
/// generic muzzle-flash light (colour (255, 192, 64) exponent 5, radius 32
/// to 64 units, 0.05 s), scaled down to 1/16 of that colour (our choice)
/// so it lights nearby walls brightly without burning them out.
pub fn muzzle_flash(materials: &mut super::material::MaterialLoader) -> MapMuzzleFlash {
    let exp = 2f32.powi(5) / 255.0 / 16.0;
    MapMuzzleFlash {
        texture: materials.resolve("effects/muzzleflashx").texture,
        sprites: vec![(0.0, 8.0 * UNIT), (3.0 * UNIT, 6.5 * UNIT), (6.0 * UNIT, 5.0 * UNIT)],
        scale: (0.8, 1.2),
        color: [1.0, 1.0, 1.0],
        life: 0.05,
        light: Some(MapFlashLight {
            color: Vec3::new(255.0, 192.0, 64.0) * exp,
            radius: (32.0 * UNIT, 64.0 * UNIT),
            life: 0.05,
        }),
    }
}

/// The knife has no idle keys, and CS:S's holds still at least 10 s after
/// it is drawn (captures 2, 6 and 10 s after drawing it, identical: the
/// draw's last frame, which the idle's still first quarter matches),
/// where its 12.5 s idle would turn the blade from 3.4 s into it. So it
/// idles this long after its draw or last attack, and as often: a guess
/// longer than the captures (docs/tech-debt.md). Its data's `[anim]`.
pub const KNIFE_IDLE: f32 = 20.0;

pub const DRAW: &str = "ACT_VM_DRAW";
pub const IDLE: &str = "ACT_VM_IDLE";
pub const PRIMARY: &str = "ACT_VM_PRIMARYATTACK";
pub const SECONDARY: &str = "ACT_VM_SECONDARYATTACK";
pub const RELOAD: &str = "ACT_VM_RELOAD";
pub const DRYFIRE: &str = "ACT_VM_DRYFIRE";
/// Dual pistols: the last round from either hand.
pub const DRYFIRE_LEFT: &str = "ACT_VM_DRYFIRE_LEFT";
/// Shotguns (spec weapons.md 3.4): the reload's start and finish; each
/// shell going in plays `RELOAD`.
pub const SHOTGUN_RELOAD_START: &str = "ACT_SHOTGUN_RELOAD_START";
pub const SHOTGUN_RELOAD_FINISH: &str = "ACT_SHOTGUN_RELOAD_FINISH";
pub const ATTACH_SILENCER: &str = "ACT_VM_ATTACH_SILENCER";
pub const DETACH_SILENCER: &str = "ACT_VM_DETACH_SILENCER";
/// Silenced weapons' view models tag their silenced set with this suffix
/// (spec weapons.md, view-model durations).
const SILENCED: &str = "_SILENCED";
pub const HIT_CENTER: &str = "ACT_VM_HITCENTER";
/// Grenades (spec grenades.md 2): the pin pull (the unused pull variants
/// are the same sequence) and the throw.
pub const PULL_PIN: &str = "ACT_VM_PULLPIN";
pub const PULL_BACK: &str = "ACT_VM_PULLBACK_HIGH";
pub const THROW: &str = "ACT_VM_THROW";
pub const MISS_CENTER: &str = "ACT_VM_MISSCENTER";

/// `v_knife_t.mdl` tags the stab and both slashes `ACT_VM_HITCENTER` and
/// the stab miss `ACT_VM_MISSCENTER`, so its attacks are picked by
/// sequence name (spec 6.2 lists them; which the game plays when is open:
/// slashes are chosen at random here, hit or miss).
const KNIFE_SLASH: &[&str] = &["midslash1", "midslash2"];
const KNIFE_STAB: &[&str] = &["stab"];
const KNIFE_STAB_MISS: &[&str] = &["stab_miss"];

pub struct ViewAnimPlugin;

impl Plugin for ViewAnimPlugin {
    fn build(&self, app: &mut App) {
        if !app.is_plugin_added::<view_anim::ViewAnimPlugin>() {
            app.add_plugins(view_anim::ViewAnimPlugin);
        }
        app.insert_resource(settings())
            .insert_resource(rules())
            .init_resource::<ViewMotion>()
            .add_systems(Update, view_motion::update.in_set(DriveAnimation));
        cvars(app);
    }
}

/// The view model's motion cvars (`cl_bobcycle`, `cl_bobup`,
/// `cl_wpn_sway_interp`, `cl_wpn_sway_scale`) and `ViewMotion`'s system
/// (`view_motion::update`), for an app that plays CS:S's guns beside
/// another game's (the party): the local player's bob and sway, after the
/// other game's view-model driver, only while that driver has let go of
/// the character (`ours` says whether it has not).
pub fn add_motion<M>(app: &mut App, ours: impl SystemCondition<M>) {
    app.init_resource::<ViewMotion>()
        .add_systems(Update, view_motion::update.after(DriveAnimation).run_if(ours));
    motion_cvars(app);
}

fn motion_cvars(app: &mut App) {
    resource_cvar::<ViewMotion, f32>(
        app,
        "cl_bobcycle",
        "View model bob period, in bob time (distance / 320 units).",
        |m| &mut m.bobcycle,
    );
    resource_cvar::<ViewMotion, f32>(app, "cl_bobup", "Rising fraction of a bob cycle.", |m| &mut m.bobup);
    resource_cvar::<ViewMotion, f32>(
        app,
        "cl_wpn_sway_interp",
        "Seconds the view model lags behind view rotation (0: no sway).",
        |m| &mut m.sway_interp,
    );
    resource_cvar::<ViewMotion, f32>(app, "cl_wpn_sway_scale", "View model sway gain.", |m| &mut m.sway_scale);
}

fn cvars(app: &mut App) {
    resource_cvar::<ViewModelSettings, f32>(
        app,
        "viewmodel_fov",
        "View model field of view (horizontal degrees at 4:3, scaled to the screen; zooms with the view).",
        |s| &mut s.fov,
    );
    resource_cvar::<ViewModelSettings, u8>(
        app,
        "cl_righthand",
        "1: weapons in the right hand (left-handed models are mirrored); 0: left.",
        |s| &mut s.right_hand,
    );
    resource_cvar::<ViewModelSettings, u8>(app, "r_drawviewmodel", "0: no view model.", |s| &mut s.draw);
    motion_cvars(app);
    resource_cvar::<EffectSettings, u8>(
        app,
        "muzzleflash_light",
        "1: muzzle flashes light the world, props and view models.",
        |e| &mut e.muzzle_light,
    );
    resource_cvar::<EffectSettings, u8>(app, "cl_ejectbrass", "1: weapons eject shells.", |e| &mut e.eject_brass);
    let mut console = app.world_mut().resource_mut::<Console>();
    for name in ["viewmodel_fov", "cl_righthand", "muzzleflash_light"] {
        console.archive(name);
    }
}

/// `activities`, each preceded by its silenced variant when `silenced`.
fn variants(activities: &[&str], silenced: bool) -> Pick {
    Pick::Activities(
        activities
            .iter()
            .flat_map(|a| {
                silenced
                    .then(|| format!("{a}{SILENCED}"))
                    .into_iter()
                    .chain([a.to_string()])
            })
            .collect(),
    )
}

/// Silenced sequences in a silencer's mode (other modes' models have none,
/// so this falls back to the plain ones).
fn silenced(state: &ViewState) -> bool {
    state.mode > 0
}

/// CS:S's view-model rules on Chimera's driver (`weapon::view_anim`).
/// When the weapon in hand idles is its data's `[anim]` (the script's
/// `TimeToIdle`, spec weapons.md 3.7, legacy keys table; `IdleInterval`,
/// the spec's "probably" reading: only non-looping idles that move show
/// it, the TMP's 5 s sway, `dump --sequences` lists each idle's motion;
/// whether the draw counts as an attack: the knife).
pub fn rules() -> ViewAnimRules {
    ViewAnimRules {
        // A new weapon in hand draws it (spec 3.3).
        draw: |s| variants(&[DRAW, IDLE], silenced(s)),
        idle: |s| variants(&[IDLE], silenced(s)),
        event,
        // Spec view_models.md 6.
        effect: event_effect,
    }
}

fn acts(list: &[&str]) -> Pick {
    Pick::activities(list)
}

/// What CS:S's view model plays on a weapon event (spec weapons.md
/// 3.3–3.8).
fn event(f: &mut ViewFrame, kind: &WeaponEventKind) {
    let silenced = silenced(&f.state);
    let clip = f.state.clip;
    let empty = clip == Some(0);
    let dual = super::weapons::alternate_hands(f.state.weapon);
    match kind {
        WeaponEventKind::Shot { .. } if !f.fired => {
            if let Some(b) = f.state.burst {
                // One sequence per burst (the Glock's burst fire).
                let length = b.interval as f64 * b.count.saturating_sub(1) as f64;
                if f.now - f.dice.burst_started > length + 0.01 {
                    f.dice.burst_started = f.now;
                    f.fired = f.play(&acts(&[SECONDARY, PRIMARY]));
                }
            } else if dual {
                // Dual pistols fire hand by hand (spec weapons.md,
                // view-model table: left the primary activity, right the
                // secondary), the last round its own.
                let right = super::weapons::elite_right_hand(clip.unwrap_or(0));
                let order: &[&str] = match (empty, right) {
                    (true, true) => &[DRYFIRE, SECONDARY],
                    (true, false) => &[DRYFIRE_LEFT, PRIMARY],
                    (false, true) => &[SECONDARY],
                    (false, false) => &[PRIMARY],
                };
                f.fired = f.play(&acts(order));
            } else {
                // The last round has its own sequence on pistols.
                let order: &[&str] = if empty { &[DRYFIRE, PRIMARY] } else { &[PRIMARY] };
                f.fired = f.play(&variants(order, silenced));
            }
            f.dice.last_attack = f.now;
        }
        WeaponEventKind::ModeChanged { mode } => {
            // Silencers have sequences for it; scopes and bursts don't.
            let act = if *mode > 0 { ATTACH_SILENCER } else { DETACH_SILENCER };
            if f.play(&acts(&[act])) {
                f.dice.last_attack = f.now;
            }
        }
        WeaponEventKind::Swing { hit, secondary, .. } => {
            // Slashes have no miss sequence (spec 6.2): the hit ones play
            // either way.
            let (names, order): (&[&str], &[&str]) = match (secondary, hit) {
                (false, _) => (KNIFE_SLASH, &[HIT_CENTER, PRIMARY]),
                (true, true) => (KNIFE_STAB, &[HIT_CENTER, SECONDARY]),
                (true, false) => (KNIFE_STAB_MISS, &[MISS_CENTER, SECONDARY]),
            };
            let named = Pick::Names(names.iter().map(|n| n.to_string()).collect());
            if !f.play(&named) {
                f.play(&acts(order));
            }
            f.dice.last_attack = f.now;
        }
        WeaponEventKind::ReloadStarted => {
            let order: &[&str] = if f.state.shells {
                &[SHOTGUN_RELOAD_START]
            } else {
                &[RELOAD]
            };
            f.attack(&variants(order, silenced));
        }
        WeaponEventKind::ShellInserting => {
            f.attack(&acts(&[RELOAD]));
        }
        WeaponEventKind::Reloaded if f.state.shells => {
            f.attack(&acts(&[SHOTGUN_RELOAD_FINISH]));
        }
        WeaponEventKind::PinPulled => {
            f.attack(&acts(&[PULL_PIN, PULL_BACK]));
        }
        WeaponEventKind::Thrown => {
            f.attack(&acts(&[THROW]));
        }
        // The C4's key presses (spec objectives.md: `pressbutton`, the
        // primary activity); stopping short goes back to idle.
        WeaponEventKind::ArmingStarted => {
            f.attack(&acts(&[PRIMARY]));
        }
        WeaponEventKind::ArmingStopped => {
            f.play(&acts(&[IDLE]));
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn view_model_cvars_set_and_archive() {
        let mut app = App::new();
        app.add_plugins(crate::console::ConsolePlugin)
            .insert_resource(settings())
            .init_resource::<EffectSettings>()
            .init_resource::<ViewMotion>();
        cvars(&mut app);
        app.world_mut()
            .resource_mut::<Console>()
            .submit("viewmodel_fov 68; cl_righthand 0; r_drawviewmodel 0; cl_bobcycle 0.4; cl_wpn_sway_scale 20");
        app.update();
        let s = *app.world().resource::<ViewModelSettings>();
        assert_eq!((s.fov, s.right_hand, s.draw), (68.0, 0, 0));
        let m = *app.world().resource::<ViewMotion>();
        assert_eq!((m.bobcycle, m.sway_scale), (0.4, 20.0));
        let console = app.world().resource::<Console>();
        assert!(console.cvar("cl_righthand").unwrap().archive);
        assert!(console.cvar("viewmodel_fov").unwrap().archive);
        assert_eq!(console.cvar("cl_righthand").unwrap().default, "1");
        assert_eq!(console.cvar("viewmodel_fov").unwrap().default, "80");
    }
}
