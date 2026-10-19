//! Combat Arms' attribute records: what the game builds from its attribute
//! files (read by `lithtech_formats::attributes`), per
//! specs/combat_arms/attribute_data.md. Each record starts from the
//! spec's built-in defaults ("Default" columns) and takes a file value
//! only when it has an accepted type ("Typed reads").
//!
//! - `WeaponsFile`: the weapons file (`ATTRIBUTES/WEAPONS.TXT`): armour
//!   (`Gear#`), ammo (`Ammo#`), weapons (`Weapon#`, ammo linked by name),
//!   `[WeaponOrder]` and `[Perturb]`, in the game's load order and with
//!   its load failures ("Weapons file: record kinds and load order").
//! - `ServerPlayer`: `[Player]` in the server attributes file
//!   (`SERVERBUTES.TXT`).
//! - `ModelsFile`: body types, characters and head pieces
//!   (`MODELBUTES.TXT`).
//! - `ModFile`: the modification file (`MOD.TXT`,
//!   specs/combat_arms/attachments.md): attachment records (`Mod#`, as
//!   `attachments::ModRecord`) and each weapon's accepted list
//!   (`Weapon#`, unknown names dropped with their variant index).
//!
//! Names are kept as text (ASCII in shipped data; other bytes as
//! Latin-1); `$` strings are kept as they are.
//!
//! Known shortcuts:
//! - Only the fields the spec's tables list are read (weapon presentation
//!   strings and vectors, crosshair styles, `[BombExceptList]`,
//!   conversion, pattern and editor lists are skipped: not needed for the
//!   slice).
//! - `DashGauge` and `GaugeRec` have no stated default ("none"); like
//!   `Maxstamina` ("none (very negative)") they fall back to the smallest
//!   32-bit integer.
//! - `DamageRFacter` is stored as read; `WeaponRecord::damage_roll`
//!   applies the "outside [0, 1] becomes 0.2 when used" rule.
//! - Ammo `Type` has default "unknown": -1 here.

use lithtech_formats::attributes::{Document, Section, c_atof, c_atoi};

use crate::game::attachments::ModRecord;

/// The four files a loader for this spec needs, in-archive paths
/// ("Where the data sits"), as the mount names them.
pub const WEAPONS_FILE: &str = "attributes/weapons.txt";
pub const SERVER_FILE: &str = "attributes/serverbutes.txt";
pub const MODELS_FILE: &str = "attributes/modelbutes.txt";
pub const MOD_FILE: &str = "attributes/mod.txt";

/// The "none" fallback of `Maxstamina`, `DashGauge`, `GaugeRec`.
pub const NO_INT: i32 = i32::MIN;

fn name_of(s: &Section) -> String {
    s.fixed_string("Name", "")
}

/// The case-insensitive, first-match name link ("Links between
/// records"): an empty name never matches.
pub fn find_by_name<'a, T>(items: &'a [T], name: &str, name_of: impl Fn(&T) -> &str) -> Option<(usize, &'a T)> {
    if name.is_empty() {
        return None;
    }
    items.iter().enumerate().find(|(_, t)| {
        let n = name_of(t);
        !n.is_empty() && n.eq_ignore_ascii_case(name)
    })
}

fn text(bytes: &[u8]) -> String {
    bytes.iter().map(|&b| b as char).collect()
}

/// `Gear<N>`: armour.
#[derive(Clone, Debug, PartialEq)]
pub struct GearRecord {
    pub name: String,
    pub armor: f64,
    pub health: f64,
    pub exclusive: i32,
    pub protection_type: Vec<i32>,
    /// Fractions absorbed, one per protection type, clamped to [0, 1].
    pub head: Vec<f64>,
    pub torso: Vec<f64>,
    pub arm: Vec<f64>,
    pub leg: Vec<f64>,
    pub consumable: Vec<i32>,
}

impl GearRecord {
    pub fn read(s: &Section) -> Self {
        let fractions = |k: &str| s.number_list(k).into_iter().map(|v| v.clamp(0.0, 1.0)).collect();
        Self {
            name: name_of(s),
            armor: s.number("Armor", 0.0),
            health: s.number("Health", 0.0),
            exclusive: s.int("Exclusive", 1),
            protection_type: s.int_list("ProtectionType"),
            head: fractions("NodeHeadProtection"),
            torso: fractions("NodeTorsoProtection"),
            arm: fractions("NodeArmProtection"),
            leg: fractions("NodelegProtection"),
            consumable: s.int_list("Consumable"),
        }
    }
}

/// The ammo's spread values ("Field table: ammo"), plain or `Zoomed`.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct AmmoPerturb {
    pub fire: f64,
    pub fire_move: f64,
    pub fire_duck: f64,
    pub fire_move_duck: f64,
    pub walk: f64,
    pub run: f64,
    pub jump: f64,
    pub recoil: f64,
}

impl AmmoPerturb {
    fn read(s: &Section, prefix: &str) -> Self {
        let n = |k: &str| s.number(&format!("{prefix}{k}"), 0.0);
        Self {
            fire: n("FirePerturb"),
            fire_move: n("FireMovePerturb"),
            fire_duck: n("FireDuckPerturb"),
            fire_move_duck: n("FireMoveDuckPerturb"),
            walk: n("WalkPerturb"),
            run: n("RunPerturb"),
            jump: n("JumpPerturb"),
            recoil: n("PerturbRecoil"),
        }
    }
}

/// `Ammo<N>`.
#[derive(Clone, Debug, PartialEq)]
pub struct AmmoRecord {
    pub name: String,
    pub kind: i32,
    pub priority: f64,
    pub max_amount: i32,
    pub spawned_amount: i32,
    pub selection_amount: i32,
    pub inst_damage: i32,
    pub alt_inst_damage: i32,
    pub inst_damage_type: i32,
    /// `Rangedamage0..`, converted from strings as decimals.
    pub range_damage: Vec<f64>,
    pub alt_range_damage: Vec<f64>,
    pub area_damage: i32,
    pub area_damage_radius: i32,
    pub area_damage_type: i32,
    pub prog_damage: f64,
    pub prog_damage_duration: f64,
    pub prog_damage_radius: f64,
    pub prog_damage_lifetime: f64,
    pub prog_damage_type: i32,
    pub stunned_time: f64,
    pub stunned_move_penalty: f64,
    pub can_adjust_inst_damage: i32,
    pub perturb: AmmoPerturb,
    pub zoomed_perturb: AmmoPerturb,
    /// ms.
    pub dec_delay: i32,
    /// ms.
    pub jitter_dec_delay: i32,
    pub jitter_wait_recover: f64,
    pub jitter_recover: f64,
    pub damage_move_penalty: f64,
    pub damage_move_penalty_duration: f64,
    pub health: f64,
}

impl AmmoRecord {
    pub fn read(s: &Section) -> Self {
        let inst_damage = s.int("InstDamage", 0);
        let bands = |k: &str| s.string_list(k).iter().map(|v| c_atof(v)).collect();
        Self {
            name: name_of(s),
            kind: s.int("Type", -1),
            priority: s.number("Priority", -1.0),
            max_amount: s.int("MaxAmount", 0),
            spawned_amount: s.int("SpawnedAmount", 0),
            selection_amount: s.int("SelectionAmount", 0),
            inst_damage,
            alt_inst_damage: s.int("Alt_InstDamage", inst_damage),
            inst_damage_type: s.int("InstDamageType", -1),
            range_damage: bands("Rangedamage"),
            alt_range_damage: bands("Alt_Rangedamage"),
            area_damage: s.int("AreaDamage", 0),
            area_damage_radius: s.int("AreaDamageRadius", 0),
            area_damage_type: s.int("AreaDamageType", -1),
            prog_damage: s.number("ProgDamage", 0.0),
            prog_damage_duration: s.number("ProgDamageDuration", 0.0),
            prog_damage_radius: s.number("ProgDamageRadius", 0.0),
            prog_damage_lifetime: s.number("ProgDamageLifetime", 0.0),
            prog_damage_type: s.int("ProgDamageType", -1),
            stunned_time: s.number("StunnedTime", 0.0),
            stunned_move_penalty: s.number("StunnedMovePenalty", 0.0),
            can_adjust_inst_damage: s.int("CanAdjustInstDamage", 1),
            perturb: AmmoPerturb::read(s, ""),
            zoomed_perturb: AmmoPerturb::read(s, "Zoomed"),
            // A wrong type keeps an undefined value in the game: the
            // missing-key default here ("Edge cases").
            dec_delay: s.int("DecDelay", 150),
            jitter_dec_delay: s.int("JitterDecDelay", 600),
            jitter_wait_recover: s.number("JitterWaitRecover", 0.1),
            jitter_recover: s.number("JitterRecover", 0.3),
            damage_move_penalty: s.number("DamageMovePenalty", 0.0),
            damage_move_penalty_duration: s.number("DamageMovePenaltyDuration", -1.0),
            health: s.number("Health", 0.0),
        }
    }
}

/// A weapon's spread limits and rates (plain, zoomed or burst).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Cone {
    pub min: i32,
    pub max: i32,
    pub inc: f64,
    pub dec: f64,
}

/// `Weapon<N>`.
#[derive(Clone, Debug, PartialEq)]
pub struct WeaponRecord {
    pub name: String,
    pub gun_type: i32,
    pub client_weapon_type: i32,
    pub ani_type: i32,
    pub lower_ani_type: i32,
    pub shots_per_clip: i32,
    /// Ammo record IDs, the names that resolved (first: the default).
    pub ammo: Vec<usize>,
    /// Whether `AmmoName0` existed at all (for "no resolvable ammo").
    pub names_ammo: bool,
    pub infinite_ammo: i32,
    pub reload_per_bullet: i32,
    /// `ReloadAfterFire` (weapon_classes.md: leave zoom after a zoomed
    /// shot; not in attribute_data.md's table: 0 when missing).
    pub reload_after_fire: i32,
    /// `AccumPreFire` (weapon_classes.md 3.3: a reversible spin; 0 when
    /// missing).
    pub accum_pre_fire: i32,
    /// `DefaultScopeAttach`: the scope fitted at spawn, by name (the
    /// modifications file's; empty when missing). The other slots'
    /// defaults: `DefaultMuzzleAttach`, `DefaultMagazineAttach`,
    /// `DefaultBulletAttach`, `DefaultMechSightAttach` (attachments.md).
    pub default_scope_attach: String,
    pub default_muzzle_attach: String,
    pub default_magazine_attach: String,
    pub default_bullet_attach: String,
    pub default_mech_sight_attach: String,
    /// `DefaultScopeVisible` (default 1).
    pub default_scope_visible: bool,
    pub vectors_per_round: i32,
    /// World units.
    pub range: i32,
    /// `Effectrange0..`: band limits in world units (strings truncated).
    pub effect_range: Vec<i32>,
    pub alt_effect_range: Vec<i32>,
    pub damage_r_facter: f64,
    /// ms, negative values already 0.
    pub fire_delay: i32,
    pub fire_ani_rate: f64,
    pub rapid_fire_count: i32,
    pub continuous_fire: i32,
    pub hud_fire_mode: i32,
    pub triple_shot_mode: i32,
    pub triple_shot_round_num: i32,
    pub triple_shot_delay: i32,
    pub triple_shot_select_delay: i32,
    pub triple_shot: Cone,
    pub triple_shot_max_cam_recoil_pitch: f64,
    pub triple_shot_base_cam_recoil_pitch: f64,
    pub perturb: Cone,
    pub duck_perturb: i32,
    pub move_duck_perturb: i32,
    pub zoomed: Cone,
    pub dynamic_perturb_factor: f64,
    pub dynamic_perturb_factor_run: f64,
    pub dynamic_perturb_factor_walk: f64,
    pub dynamic_perturb_factor_jump: f64,
    pub zoom_time: f64,
    pub move_penalty: f64,
    pub zoomed_move_penalty: f64,
    pub max_cam_recoil_pitch: f64,
    pub base_cam_recoil_pitch: f64,
    pub base_cam_recoil_accel_pitch: f64,
    pub max_cam_recoil_yaw: f64,
    pub base_cam_recoil_yaw: f64,
    pub max_cam_recoil_wag: f64,
    pub base_cam_recoil_wag_l: f64,
    pub base_cam_recoil_wag_r: f64,
    pub max_random_pitch: f64,
    pub max_random_wag: f64,
    pub base_cam_back_jitter: f64,
    pub back_jitter_recover: f64,
    pub camera_sway: [f64; 4],
    pub disable_crosshair: i32,
    pub cross_hair_type: i32,
    pub no_select: i32,
    pub grenade_speed: f64,
    pub grenade_attach: i32,
    pub wide_fire_max_num: i32,
    pub wide_fire_angle_degree: i32,
    pub fire_snd_radius: i32,
    pub reload_snd_radius: i32,
    pub select_snd_radius: i32,
    /// `SilencedFireRadius1..10`.
    pub silenced_fire_radius: [i32; 10],
    pub drop_by_player_dead: bool,
    pub damage_weapon: bool,
    pub check_reload_ani_time: bool,
    pub hidden: bool,
}

impl WeaponRecord {
    /// Read a weapon, linking its `AmmoName#` list to `ammo` by name.
    pub fn read(s: &Section, ammo: &[AmmoRecord]) -> Self {
        let n = |k: &str, d: f64| s.number(k, d);
        let i = |k: &str, d: i32| s.int(k, d);
        let names = s.string_list("AmmoName");
        let ammo_ids = names
            .iter()
            .take(10)
            .filter_map(|name| find_by_name(ammo, &text(name), |a| &a.name).map(|(id, _)| id))
            .collect();
        let bands = |k: &str| s.string_list(k).iter().map(|v| c_atoi(v)).collect();
        let mut silenced = [0; 10];
        for (k, r) in silenced.iter_mut().enumerate() {
            *r = i(&format!("SilencedFireRadius{}", k + 1), 0);
        }
        Self {
            name: name_of(s),
            gun_type: i("Guntype", 0),
            client_weapon_type: i("ClientWeaponType", 1),
            ani_type: i("AniType", 5),
            lower_ani_type: i("LowerAniType", 0),
            shots_per_clip: i("ShotsPerClip", 100),
            ammo: ammo_ids,
            names_ammo: !names.is_empty(),
            infinite_ammo: i("InfiniteAmmo", 0),
            // Undefined in the game when missing ("Edge cases"): 0.
            reload_per_bullet: i("ReloadPerBullet", 0),
            reload_after_fire: i("ReloadAfterFire", 0),
            accum_pre_fire: i("AccumPreFire", 0),
            default_scope_attach: s.fixed_string("DefaultScopeAttach", ""),
            default_muzzle_attach: s.fixed_string("DefaultMuzzleAttach", ""),
            default_magazine_attach: s.fixed_string("DefaultMagazineAttach", ""),
            default_bullet_attach: s.fixed_string("DefaultBulletAttach", ""),
            default_mech_sight_attach: s.fixed_string("DefaultMechSightAttach", ""),
            default_scope_visible: s.int("DefaultScopeVisible", 1) != 0,
            vectors_per_round: i("VectorsPerRound", 1),
            range: i("Range", 3000),
            effect_range: bands("Effectrange"),
            alt_effect_range: bands("Alt_Effectrange"),
            damage_r_facter: n("DamageRFacter", 0.1),
            fire_delay: i("FireDelay", 30).max(0),
            fire_ani_rate: n("FireAniRate", 0.0),
            rapid_fire_count: i("RapidFireCount", 1),
            continuous_fire: i("ContinuousFire", 0),
            hud_fire_mode: i("HudFireMode", 0),
            triple_shot_mode: i("TripleShotMode", 0),
            triple_shot_round_num: i("TripleShotRoundNum", 3),
            triple_shot_delay: i("TripleShotDelay", 300),
            triple_shot_select_delay: i("TripleShotSelectDelay", 400),
            triple_shot: Cone {
                min: i("TripleShotMinPerturb", 7),
                max: i("TripleShotMaxPerturb", 100),
                inc: n("TripleShotPerturbIncSpeed", 1.0),
                dec: n("TripleShotPerturbDecSpeed", 1.0),
            },
            triple_shot_max_cam_recoil_pitch: n("TripleShotMaxCamRecoilPitch", 50.0),
            triple_shot_base_cam_recoil_pitch: n("TripleShotBaseCamRecoilPitch", 1.0),
            perturb: Cone {
                min: i("MinPerturb", 100),
                max: i("MaxPerturb", 100),
                inc: n("PerturbIncSpeed", 1.0),
                dec: n("PerturbDecSpeed", 1.0),
            },
            duck_perturb: i("DuckPerturb", 100),
            move_duck_perturb: i("MoveDuckPerturb", 100),
            zoomed: Cone {
                min: i("ZoomedMinPerturb", 1),
                max: i("ZoomedMaxPerturb", 500),
                inc: n("ZoomedPerturbIncSpeed", 3.0),
                dec: n("ZoomedPerturbDecSpeed", 3.0),
            },
            dynamic_perturb_factor: n("DynamicPerturbFactor", 1.0),
            dynamic_perturb_factor_run: n("DynnamicPerturbFactorRun", 1.0),
            dynamic_perturb_factor_walk: n("DynnamicPerturbFactorWalk", 1.0),
            dynamic_perturb_factor_jump: n("DynnamicPerturbFactorJump", 1.0),
            zoom_time: n("ZoomTime", 0.5),
            move_penalty: n("MovePenalty", 0.01),
            zoomed_move_penalty: n("ZoomedMovePenalty", 0.7),
            max_cam_recoil_pitch: n("MaxCamRecoilPitch", 10.0),
            base_cam_recoil_pitch: n("BaseCamRecoilPitch", 1.2),
            base_cam_recoil_accel_pitch: n("BaseCamRecoilAccelPitch", 0.0),
            max_cam_recoil_yaw: n("MaxCamRecoilYaw", 0.0),
            base_cam_recoil_yaw: n("BaseCamRecoilYaw", 0.0),
            max_cam_recoil_wag: n("MaxCamRecoilWag", 30.0),
            base_cam_recoil_wag_l: n("BaseCamRecoilWagL", -1.0),
            base_cam_recoil_wag_r: n("BaseCamRecoilWagR", 1.0),
            max_random_pitch: n("MaxRandomPitch", 0.032),
            max_random_wag: n("MaxRandomWag", 0.015),
            base_cam_back_jitter: n("BaseCamBackJitter", 0.5),
            back_jitter_recover: n("BackJitterRecover", 3.0),
            camera_sway: [
                n("CameraSwayXSpeed", 12.0),
                n("CameraSwayYSpeed", 2.0),
                n("CameraSwayXFreq", 6.0),
                n("CameraSwayYFreq", 20.0),
            ],
            disable_crosshair: i("DisableCrosshair", 0),
            cross_hair_type: i("CrossHairType", 0),
            no_select: i("NoSelect", 0),
            grenade_speed: n("GrenadeSpeed", 1.0),
            grenade_attach: i("GrenadeAttach", 0),
            wide_fire_max_num: i("WideFireMaxNum", 0),
            wide_fire_angle_degree: i("WideFireAngleDegree", 0),
            fire_snd_radius: i("FireSndRadius", 1000),
            reload_snd_radius: i("ReloadSndRadius", 300),
            select_snd_radius: i("SelectSndRadius", 300),
            silenced_fire_radius: silenced,
            drop_by_player_dead: s.bool("DropByPlayerDead", true),
            damage_weapon: s.bool("DamageWeapon", true),
            check_reload_ani_time: s.bool("CheckReloadAniTime", true),
            hidden: s.bool("Hidden", false),
        }
    }

    /// The default ammo's ID (the first that resolved).
    pub fn default_ammo(&self) -> Option<usize> {
        self.ammo.first().copied()
    }

    /// The random damage spread as used: outside [0, 1] it is 0.2.
    pub fn damage_roll(&self) -> f64 {
        if (0.0..=1.0).contains(&self.damage_r_facter) {
            self.damage_r_facter
        } else {
            0.2
        }
    }
}

/// The damage-by-range multiplier of a weapon with an ammo ("Damage by
/// range"): bands only when both lists have at least three elements.
pub fn range_multiplier(effect_range: &[i32], range_damage: &[f64], distance: f64) -> f64 {
    let (Some(e), Some(r)) = (effect_range.get(..3), range_damage.get(..3)) else {
        return 1.0;
    };
    if (0.0..=e[0] as f64).contains(&distance) {
        r[0]
    } else if distance > e[0] as f64 && distance <= e[1] as f64 {
        r[1]
    } else if distance > e[1] as f64 && distance <= e[2] as f64 {
        r[2]
    } else {
        1.0
    }
}

/// `[Perturb]`: global spread speeds, each read only if present.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PerturbGlobals {
    pub movement_increase: f64,
    pub movement_decrease: f64,
    pub firing_increase: f64,
    pub firing_decrease: f64,
    pub rotation_effect: f64,
}

impl Default for PerturbGlobals {
    fn default() -> Self {
        Self {
            movement_increase: 3.0,
            movement_decrease: 0.7,
            firing_increase: 4.0,
            firing_decrease: 1.0,
            rotation_effect: 2.0,
        }
    }
}

/// The weapons file, built as the game builds it.
#[derive(Clone, Debug, PartialEq)]
pub struct WeaponsFile {
    pub gear: Vec<GearRecord>,
    pub ammo: Vec<AmmoRecord>,
    pub weapons: Vec<WeaponRecord>,
    /// `[WeaponOrder]`: each class name and its weapons' IDs in order.
    pub order: Vec<(String, Vec<usize>)>,
    /// `AutoSwitchWeapon0..` as weapon IDs (unknown names dropped).
    pub auto_switch: Vec<usize>,
    pub perturb: PerturbGlobals,
}

impl WeaponsFile {
    /// Build the records; `Err` where the game fails to load (no ammo, no
    /// weapons, an order list naming no weapon).
    pub fn read(doc: &Document) -> Result<Self, String> {
        let gear = doc.records("Gear").into_iter().map(GearRecord::read).collect();
        let ammo: Vec<_> = doc.records("Ammo").into_iter().map(AmmoRecord::read).collect();
        if ammo.is_empty() {
            return Err("no Ammo0 record".into());
        }
        let weapons: Vec<_> = doc.records("Weapon").into_iter().map(|s| WeaponRecord::read(s, &ammo)).collect();
        if weapons.is_empty() {
            return Err("no Weapon0 record".into());
        }
        let weapon_id = |name: &[u8]| find_by_name(&weapons, &text(name), |w| &w.name).map(|(id, _)| id);
        let mut order = Vec::new();
        let mut auto_switch = Vec::new();
        if let Some(s) = doc.section("WeaponOrder") {
            for class in s.string_list("Class") {
                let class = text(&class);
                let mut ids = Vec::new();
                for name in s.string_list(&class) {
                    ids.push(weapon_id(&name).ok_or_else(|| {
                        format!("[WeaponOrder] {class}: no weapon named \"{}\"", text(&name))
                    })?);
                }
                order.push((class, ids));
            }
            auto_switch = s.string_list("AutoSwitchWeapon").iter().filter_map(|n| weapon_id(n)).collect();
        }
        let mut perturb = PerturbGlobals::default();
        if let Some(s) = doc.section("Perturb") {
            for (key, field) in [
                ("MovementPerturbIncreaseSpeed", &mut perturb.movement_increase),
                ("MovementPerturbDecreaseSpeed", &mut perturb.movement_decrease),
                ("FiringPerturbIncreaseSpeed", &mut perturb.firing_increase),
                ("FiringPerturbDecreaseSpeed", &mut perturb.firing_decrease),
                ("PerturbRotationEffect", &mut perturb.rotation_effect),
            ] {
                *field = s.number(key, *field);
            }
        }
        Ok(Self {
            gear,
            ammo,
            weapons,
            order,
            auto_switch,
            perturb,
        })
    }

    /// Every class's weapon IDs, one list in class order.
    pub fn order_flat(&self) -> Vec<usize> {
        self.order.iter().flat_map(|(_, ids)| ids.iter().copied()).collect()
    }

    /// A weapon's inventory class: the 1-based place of the order list it
    /// is in (`Class0` is 1, primary; ... 6, mission), 0 in none. Not its
    /// `Guntype`: the slot keys and the HUD's weapon panel read this.
    pub fn inventory_class(&self, weapon: usize) -> u8 {
        self.order
            .iter()
            .position(|(_, ids)| ids.contains(&weapon))
            .map_or(0, |i| (i + 1).min(u8::MAX as usize) as u8)
    }

    /// A weapon's command id: its place in all order lists in a row (the
    /// server's "better weapon" rule, inventory.md 11); None in no list.
    pub fn command_id(&self, weapon: usize) -> Option<usize> {
        self.order_flat().iter().position(|&w| w == weapon)
    }

    /// A weapon by name (the first match).
    pub fn weapon(&self, name: &str) -> Option<&WeaponRecord> {
        find_by_name(&self.weapons, name, |w| &w.name).map(|(_, w)| w)
    }

    /// A weapon's default ammo record.
    pub fn default_ammo(&self, weapon: &WeaponRecord) -> Option<&AmmoRecord> {
        weapon.default_ammo().map(|id| &self.ammo[id])
    }
}

/// `[Player]` in the server attributes file. A missing or wrongly typed
/// speed reads as the smallest positive double, 0 as a float.
#[derive(Clone, Debug, PartialEq)]
pub struct ServerPlayer {
    pub walk_speed: f32,
    pub run_speed: f32,
    pub jump_speed: f32,
    pub ladder_speed: f32,
    pub walk_footstep_volume: f32,
    pub run_footstep_volume: f32,
    pub crouch_footstep_volume: f32,
    pub default_weapon: String,
}

impl ServerPlayer {
    pub fn read(doc: &Document) -> Option<Self> {
        let s = doc.section("Player")?;
        let n = |k: &str| s.number(k, f64::MIN_POSITIVE) as f32;
        Some(Self {
            walk_speed: n("WalkSpeed"),
            run_speed: n("RunSpeed"),
            jump_speed: n("JumpSpeed"),
            ladder_speed: n("LadderSpeed"),
            walk_footstep_volume: n("WalkFootstepVolume"),
            run_footstep_volume: n("RunFootstepVolume"),
            crouch_footstep_volume: n("CrouchFootstepVolume"),
            default_weapon: s.fixed_string("DefaultWeapon", ""),
        })
    }
}

/// `FigureType<N>`: a body type.
#[derive(Clone, Debug, PartialEq)]
pub struct FigureTypeRecord {
    pub name: String,
    pub female: bool,
    pub dash_speed: f64,
    pub max_stamina: i32,
    pub dash_gauge: i32,
    pub gauge_rec: i32,
    pub variable_jump: f64,
    pub variable_speed: f64,
    pub variable_damage: f64,
    pub variable_weight: f64,
    /// Walk, Run, Jump, Fire, FireDuck, FireMoveDuck, FireMove perturbs and
    /// Recoil.
    pub perturb: AmmoPerturb,
    pub attacked_push: f64,
    pub stiff_duration: f64,
    /// HP, Speed, Jump, Shooting, Damagedown, WeaponFenalty (truncated).
    pub stats: [i32; 6],
    pub hp_inc_percent: f64,
}

impl FigureTypeRecord {
    pub fn read(s: &Section) -> Self {
        let n = |k: &str| s.number(k, 0.0);
        let p = AmmoPerturb::read(s, "");
        Self {
            name: name_of(s),
            female: s.fixed_string("Sex", "Female").eq_ignore_ascii_case("female"),
            dash_speed: s.number("DashSpeed", 1.4),
            max_stamina: s.int("Maxstamina", NO_INT),
            dash_gauge: s.int("DashGauge", NO_INT),
            gauge_rec: s.int("GaugeRec", NO_INT),
            variable_jump: n("VariableJump"),
            variable_speed: n("VariableSpeed"),
            variable_damage: n("VariableDamage"),
            variable_weight: n("VariableWeight"),
            perturb: AmmoPerturb {
                recoil: n("Recoil"),
                ..p
            },
            attacked_push: n("AttackedPush"),
            stiff_duration: n("StiffDuration"),
            stats: ["HP", "Speed", "Jump", "Shooting", "Damagedown", "WeaponFenalty"].map(|k| n(k) as i32),
            hp_inc_percent: n("HPIncPercent"),
        }
    }
}

/// The movement keys of a `Model<N>` (character) or `Hair<N>` (head
/// piece).
#[derive(Clone, Debug, PartialEq)]
pub struct WearRecord {
    pub name: String,
    /// `FigureType` (characters only; 0 for head pieces).
    pub figure_type: i32,
    pub move_penalty: f64,
    pub dash_speed: f64,
    pub max_stamina: f64,
}

impl WearRecord {
    pub fn read(s: &Section) -> Self {
        Self {
            name: name_of(s),
            figure_type: s.int("FigureType", 0),
            move_penalty: s.number("MovePenalty", 0.0),
            dash_speed: s.number("DashSpeed", 0.0),
            max_stamina: s.number("MaxStamina", 0.0),
        }
    }
}

/// The models file's body types, characters and head pieces.
#[derive(Clone, Debug, PartialEq)]
pub struct ModelsFile {
    pub figure_types: Vec<FigureTypeRecord>,
    pub models: Vec<WearRecord>,
    pub hair: Vec<WearRecord>,
}

impl ModelsFile {
    pub fn read(doc: &Document) -> Self {
        Self {
            figure_types: doc.records("FigureType").into_iter().map(FigureTypeRecord::read).collect(),
            models: doc.records("Model").into_iter().map(WearRecord::read).collect(),
            hair: doc.records("Hair").into_iter().map(WearRecord::read).collect(),
        }
    }
}


// ---------------------------------------------------------------------------
// The modification file (specs/combat_arms/attachments.md, "How
// attachments are described in the data")

/// A special function's kind or target written as a name (any case); a
/// number or an unknown name is `None` (the spec doesn't say how numbers
/// map).
fn named<T: Copy>(s: &Section, key: &str, names: &[(&str, T)]) -> Option<T> {
    let v = text(s.string(key)?);
    names.iter().find(|(n, _)| n.eq_ignore_ascii_case(&v)).map(|(_, t)| *t)
}

/// `Mod<N>`: an attachment record, as our data's (`ModRecord`); fields
/// missing or of another type are 0 (empty).
pub fn read_mod(s: &Section) -> ModRecord {
    use crate::game::attachments::{Special, SpecialKind as K, Target as T};
    let n = |k: &str| s.number(k, 0.0) as f32;
    let i = |k: &str| s.int(k, 0);
    let opt_int = |k: &str| match s.get(k) {
        Some(lithtech_formats::attributes::Value::Int(v)) => Some(*v),
        _ => None,
    };
    let column = |k: &str| (0..10).map(|c| s.int(&format!("{k}{c}"), 0)).collect::<Vec<_>>();
    let kinds = [("InstDamage", K::InstDamage), ("Stun", K::Stun), ("Knockback", K::Knockback), ("Crit", K::Crit)];
    let targets = [
        ("All", T::All),
        ("Player", T::Player),
        ("Npc", T::Npc),
        ("Human", T::Human),
        ("Zombie", T::Zombie),
        ("PH", T::PH),
        ("PZ", T::PZ),
        ("NH", T::NH),
        ("NZ", T::NZ),
    ];
    let mut special = Vec::new();
    for j in 0.. {
        let ty = format!("SFType{j}");
        if s.get(&ty).is_none() {
            break;
        }
        let Some(kind) = named(s, &ty, &kinds) else { continue };
        let target_key = format!("SFTargetType{j}");
        let target = if s.get(&target_key).is_none() {
            T::All
        } else {
            match named(s, &target_key, &targets) {
                Some(t) => t,
                None => continue,
            }
        };
        special.push(Special {
            kind,
            target,
            value: n(&format!("SFValue{j}")),
            duration: n(&format!("SFDuration{j}")),
            probability: s.number(&format!("SFProbability{j}"), 1.0) as f32,
        });
    }
    let fov: Vec<f32> = (0..10).map_while(|l| s.number_opt(&format!("ZoomFov{l}")).map(|v| v as f32)).collect();
    let socket = s.fixed_string("Socket", "");
    ModRecord {
        name: name_of(s),
        kinds: opt_int("Kinds"),
        kind: opt_int("Type"),
        integrated: i("Integrated"),
        socket: (!socket.is_empty()).then_some(socket),
        silencer_level: i("SilencerLevel"),
        flash_hider_level: i("FlashHiderLevel"),
        zoom_level: i("ZoomLevel"),
        zoom_fov: fov,
        sight_style: i("SightSystemStyle"),
        target_range: i("TargetRange"),
        inst_damage: n("Instdamage"),
        inst_damage_per: n("InstDamagePer"),
        range_per: n("RangePer"),
        fire_ani_rate: n("Fireanirate"),
        reload_ani_rate: n("Reloadanirate"),
        select_ani_rate: n("Selectanirate"),
        perturb_inc_speed: n("PerturbIncSpeed"),
        perturb_dec_speed: n("PerturbDecSpeed"),
        zoomed_perturb_inc_speed: n("ZoomedPerturbIncSpeed"),
        zoomed_perturb_dec_speed: n("ZoomedPerturbDecSpeed"),
        walk_perturb: n("WalkPerturb"),
        run_perturb: n("RunPerturb"),
        fire_perturb: n("FirePerturb"),
        fire_duck_perturb: n("FireDuckPerturb"),
        zoomed_min_perturb: n("ZoomedMinPerturb"),
        zoomed_max_perturb: n("ZoomedMaxPerturb"),
        dynamic_perturb_factor: n("DynamicPerturbFactor"),
        min_perturb_per: n("MinPerturbPer"),
        base_cam_recoil_pitch: n("BaseCamRecoilPitch"),
        base_cam_recoil_pitch_per: n("BaseCamRecoilPitchPer"),
        max_cam_recoil_pitch_per: n("MaxCamRecoilPitchPer"),
        base_cam_recoil_accel_pitch: n("BaseCamRecoilAccelPitch"),
        max_random_pitch: n("MaxRandomPitch"),
        camera_sway: [
            n("CameraSwayXSpeed"),
            n("CameraSwayYSpeed"),
            n("CameraSwayXFreq"),
            n("CameraSwayYFreq"),
        ],
        vectors_per_round_per: n("VectorsPerRoundPer"),
        shots_per_clip_plus: column("ShotsPerClipPlus"),
        max_amount_plus: column("MaxAmountPlus"),
        selection_amount_plus: column("SelectionAmountPlus"),
        special,
    }
}

/// `Weapon<N>` of the modification file: the accepted list of the weapon
/// with the same N in the weapons file (Q13: by position).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ModWeaponRecord {
    /// `Name` (informational).
    pub name: String,
    /// `ModName#` with its `ModModel#` (default 0); a name that matches no
    /// attachment is dropped with its index.
    pub accepts: Vec<(String, i32)>,
}

/// The modification file (`MOD.TXT`): attachment records and per-weapon
/// accepted lists.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ModFile {
    pub mods: Vec<ModRecord>,
    pub weapons: Vec<ModWeaponRecord>,
}

impl ModFile {
    pub fn read(doc: &Document) -> Self {
        let mods: Vec<ModRecord> = doc.records("Mod").into_iter().map(read_mod).collect();
        let weapons = doc
            .records("Weapon")
            .into_iter()
            .map(|s| {
                let accepts = s
                    .string_list("ModName")
                    .iter()
                    .enumerate()
                    .filter_map(|(k, name)| {
                        let (_, m) = find_by_name(&mods, &text(name), |m| &m.name)?;
                        Some((m.name.clone(), s.int(&format!("ModModel{k}"), 0)))
                    })
                    .collect();
                ModWeaponRecord {
                    name: name_of(s),
                    accepts,
                }
            })
            .collect();
        Self { mods, weapons }
    }
}
