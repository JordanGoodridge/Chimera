//! Combat Arms' attribute records and our loader against
//! specs/combat_arms/attribute_data.md, "Test cases" 12 to 29 (1 to 11,
//! the format's, are lithtech_formats' tests/it/attributes.rs), on
//! documents written here.

use combat_arms::game::data::{
    InstallAttributes, attributes::*, merge_movement, merge_weapon_docs, read_attributes, slug,
};
use combat_arms::game::movement::{MOVEMENT_DATA, MovementData};
use chimera_sim::weapon::data::WeaponSet;
use lithtech_formats::attributes::Document;

fn doc(text: &str) -> Document {
    Document::parse(text.as_bytes()).unwrap_or_else(|e| panic!("{text:?}: {e}"))
}

fn weapons(text: &str) -> Result<WeaponsFile, String> {
    WeaponsFile::read(&doc(text))
}

fn close(a: f64, b: f64) {
    assert!((a - b).abs() < 1e-9, "{a} != {b}");
}

/// 12. Record enumeration.
#[test]
fn case_12_record_enumeration() {
    let f = weapons(r#"[Ammo0] Name="a" [Ammo1] Name="b" [Ammo3] Name="d" [Weapon0] Name="W""#).unwrap();
    let names: Vec<_> = f.ammo.iter().map(|a| a.name.as_str()).collect();
    assert_eq!(names, ["a", "b"]);
}

/// 13. Links.
#[test]
fn case_13_links() {
    let f = weapons(
        r#"[Ammo0] Name="9mm" [Ammo1] Name="9MM"
        [Weapon0] Name="W" AmmoName0="9mm" AmmoName1="none" AmmoName2="9Mm" AmmoName4="9mm""#,
    )
    .unwrap();
    let w = f.weapon("w").unwrap();
    assert_eq!(w.ammo, [0, 0]);
    assert_eq!(w.default_ammo(), Some(0));
}

/// 14. Weapon defaults.
#[test]
fn case_14_weapon_defaults() {
    let f = weapons(r#"[Ammo0] Name="a" [Weapon0] Name="W" AmmoName0="a""#).unwrap();
    let w = &f.weapons[0];
    assert_eq!(
        (w.shots_per_clip, w.perturb.min, w.perturb.max, w.duck_perturb),
        (100, 100, 100, 100)
    );
    assert_eq!((w.zoomed.min, w.zoomed.max, w.range, w.vectors_per_round), (1, 500, 3000, 1));
    assert_eq!((w.fire_delay, w.rapid_fire_count), (30, 1));
    assert_eq!((w.move_penalty, w.zoomed_move_penalty, w.zoom_time), (0.01, 0.7, 0.5));
    assert_eq!((w.damage_r_facter, w.max_cam_recoil_pitch, w.base_cam_recoil_pitch), (0.1, 10.0, 1.2));
    assert_eq!((w.ani_type, w.client_weapon_type, w.gun_type), (5, 1, 0));
    assert!(w.drop_by_player_dead);
    assert!(w.effect_range.is_empty() && w.alt_effect_range.is_empty());
}

/// 15. Ammo defaults.
#[test]
fn case_15_ammo_defaults() {
    let f = weapons(r#"[Ammo0] Name="a" [Weapon0] Name="W""#).unwrap();
    let a = &f.ammo[0];
    assert_eq!((a.inst_damage, a.alt_inst_damage, a.inst_damage_type), (0, 0, -1));
    assert_eq!(a.priority, -1.0);
    assert_eq!((a.dec_delay, a.jitter_dec_delay), (150, 600));
    assert_eq!((a.jitter_wait_recover, a.jitter_recover), (0.1, 0.3));
    assert_eq!(a.damage_move_penalty_duration, -1.0);
    assert_eq!(a.perturb, AmmoPerturb::default());
    assert_eq!(a.zoomed_perturb, AmmoPerturb::default());
}

/// 16. Alt damage fallback.
#[test]
fn case_16_alt_damage_fallback() {
    let f = weapons(r#"[Ammo0] Name="a" InstDamage=35 [Weapon0] Name="W""#).unwrap();
    assert_eq!(f.ammo[0].alt_inst_damage, 35);
    let f = weapons(r#"[Ammo0] Name="a" InstDamage=35 Alt_InstDamage=75 [Weapon0] Name="W""#).unwrap();
    assert_eq!(f.ammo[0].alt_inst_damage, 75);
}

/// 17. Decimals in integer fields.
#[test]
fn case_17_decimals_in_integer_fields() {
    let f = weapons(
        r#"[Ammo0] Name="a" [Weapon0] Name="W" MinPerturb=3 DuckPerturb=6.5 ZoomedMaxPerturb=19.5 FireDelay=0.9 ShotsPerClip=30.0"#,
    )
    .unwrap();
    let w = &f.weapons[0];
    assert_eq!(
        (w.perturb.min, w.duck_perturb, w.zoomed.max, w.fire_delay, w.shots_per_clip),
        (3, 100, 500, 30, 100)
    );
}

/// 18. Integers in number fields.
#[test]
fn case_18_integers_in_number_fields() {
    let f = weapons(r#"[Ammo0] Name="a" [Weapon0] Name="W" MovePenalty=0 MaxCamRecoilPitch=5"#).unwrap();
    assert_eq!((f.weapons[0].move_penalty, f.weapons[0].max_cam_recoil_pitch), (0.0, 5.0));
}

/// 19. Fire delay clamp.
#[test]
fn case_19_fire_delay_clamp() {
    let f = weapons(r#"[Ammo0] Name="a" [Weapon0] Name="W" FireDelay=-20"#).unwrap();
    assert_eq!(f.weapons[0].fire_delay, 0);
}

/// 20. Bool fields.
#[test]
fn case_20_bool_fields() {
    let f = weapons(
        r#"[Ammo0] Name="a" [Weapon0] Name="A" DropByPlayerDead=FALSE [Weapon1] Name="B" DropByPlayerDead=0 Hidden=true"#,
    )
    .unwrap();
    assert!(!f.weapons[0].drop_by_player_dead);
    assert!(f.weapons[1].drop_by_player_dead);
    assert!(f.weapons[1].hidden);
    assert!(!f.weapons[0].hidden);
}

const BANDS_AMMO: &str = r#"[Ammo0] Name="a" InstDamage=35 Rangedamage0="1.0" Rangedamage1="0.7" Rangedamage2="0.35""#;

fn damage(f: &WeaponsFile, d: f64) -> f64 {
    let w = &f.weapons[0];
    let a = f.default_ammo(w).unwrap();
    a.inst_damage as f64 * range_multiplier(&w.effect_range, &a.range_damage, d)
}

/// 21. Range bands.
#[test]
fn case_21_range_bands() {
    let f = weapons(&format!(
        r#"{BANDS_AMMO} [Weapon0] Name="W" AmmoName0="a" Effectrange0="2500" Effectrange1="3000" Effectrange2="3500.9""#
    ))
    .unwrap();
    assert_eq!(f.weapons[0].effect_range, [2500, 3000, 3500]);
    for (d, m, dmg) in [
        (0.0, 1.0, 35.0),
        (2500.0, 1.0, 35.0),
        (2500.5, 0.7, 24.5),
        (3000.0, 0.7, 24.5),
        (3200.0, 0.35, 12.25),
        (3500.0, 0.35, 12.25),
        (3500.5, 1.0, 35.0),
    ] {
        let w = &f.weapons[0];
        close(range_multiplier(&w.effect_range, &f.ammo[0].range_damage, d), m);
        close(damage(&f, d), dmg);
    }
}

/// 22. Unquoted band element.
#[test]
fn case_22_unquoted_band_element() {
    let f = weapons(&format!(
        r#"{BANDS_AMMO} [Weapon0] Name="W" AmmoName0="a" Effectrange0=2500 Effectrange1="3000" Effectrange2="3500.9""#
    ))
    .unwrap();
    assert_eq!(f.weapons[0].effect_range, [0, 3000, 3500]);
    close(damage(&f, 100.0), 24.5);
    close(damage(&f, 0.0), 35.0);
}

/// 23. Short band list.
#[test]
fn case_23_short_band_list() {
    let f = weapons(&format!(
        r#"{BANDS_AMMO} [Weapon0] Name="W" AmmoName0="a" Effectrange0="2500" Effectrange2="3500.9""#
    ))
    .unwrap();
    assert_eq!(f.weapons[0].effect_range, [2500]);
    close(damage(&f, 3200.0), 35.0);
}

/// 24. Weapon order.
#[test]
fn case_24_weapon_order() {
    let text = r#"[Ammo0] Name="a" [Weapon0] Name="W" [Weapon1] Name="K"
        [WeaponOrder] Class0="Primary" Class1="Melee" Primary0="W" Melee0="K""#;
    assert_eq!(weapons(text).unwrap().order_flat(), [0, 1]);
    assert!(weapons(&text.replace(r#"Melee0="K""#, r#"Melee0="Nope""#)).is_err());
    // Auto-switch: an unknown name is ignored.
    let f = weapons(&format!(r#"{text} AutoSwitchWeapon0="K" AutoSwitchWeapon1="Nope" AutoSwitchWeapon2="W""#)).unwrap();
    assert_eq!(f.auto_switch, [1, 0]);
}

/// 25. Long string.
#[test]
fn case_25_long_string() {
    let named = |n: usize| weapons(&format!(r#"[Ammo0] Name="a" [Weapon0] Name="{}""#, "N".repeat(n))).unwrap();
    assert_eq!(named(128).weapons[0].name, "");
    assert_eq!(named(127).weapons[0].name, "N".repeat(127));
}

/// 26. Missing record kinds.
#[test]
fn case_26_missing_record_kinds() {
    assert!(weapons(r#"[Weapon0] Name="W""#).is_err());
    assert!(weapons(r#"[Ammo0] Name="a""#).is_err());
}

/// 27. Body type.
#[test]
fn case_27_body_type() {
    let m = ModelsFile::read(&doc(
        r#"[FigureType0] Name="Male" DashSpeed=1.0 Maxstamina=100 Speed=90.7 [FigureType1] Name="Female""#,
    ));
    let (male, female) = (&m.figure_types[0], &m.figure_types[1]);
    assert_eq!((male.dash_speed, male.max_stamina, male.stats[1]), (1.0, 100, 90));
    assert_eq!(female.dash_speed, 1.4);
    assert!(female.female);
    assert_eq!(female.variable_jump, 0.0);
    // "Sex": anything but Female (any case) is male; missing is female.
    let m = ModelsFile::read(&doc(r#"[FigureType0] Sex="Male" [FigureType1] Sex="FEMALE""#));
    assert!(!m.figure_types[0].female && m.figure_types[1].female);
}

/// 28. Server player.
#[test]
fn case_28_server_player() {
    let p = ServerPlayer::read(&doc(r#"[Player] WalkSpeed=90 RunSpeed=240.5 JumpSpeed="485""#)).unwrap();
    assert_eq!((p.walk_speed, p.run_speed, p.jump_speed, p.ladder_speed), (90.0, 240.5, 0.0, 0.0));
}

/// Our document for case 29: weapon `w`, clip 25, range 4000 world units.
const OUR_W: &str = r#"
id = "combat_arms:w"
units = { length = 0.01 }
slot = 0
draw = 1.0

[ammo."combat_arms:a"]
max = 50

[trigger]
automatic = true
cycle = 0.1
timing = "set"

[magazine]
clip = 25
ammo = "combat_arms:a"
reload = 2.0

[hitscan]
range = 4000.0
pellets = 1
punch_scale = 0.0

[damage]
amount = 25.0
falloff = 1.0
falloff_step = 1.0
hitgroups = { head = 1.0, chest = 1.0, stomach = 1.0, arm = 1.0, leg = 1.0 }
impulse = 0.0
"#;

fn merged_w(install: &str) -> WeaponSet {
    let (attrs, problems) = read_attributes(|name| {
        Ok(if name == WEAPONS_FILE {
            install.as_bytes().to_vec()
        } else {
            Vec::new()
        })
    });
    let docs = match &attrs.weapons {
        Some(w) => merge_weapon_docs(&[("w.toml", OUR_W)], w, 0.01).unwrap(),
        None => {
            assert!(!problems.is_empty());
            vec![("w.toml".to_string(), OUR_W.to_string())]
        }
    };
    let refs: Vec<(&str, &str)> = docs.iter().map(|(n, t)| (n.as_str(), t.as_str())).collect();
    WeaponSet::parse(&refs, &[]).unwrap()
}

/// 29. Override merge (our loader). The spec's install document has no
/// `Ammo0`, which case 26 says fails to load: one is added here.
#[test]
fn case_29_override_merge() {
    let set = merged_w(r#"[Ammo0] Name="other" [Weapon0] Name="W" ShotsPerClip=30"#);
    let w = &set.weapons[0];
    assert_eq!(w.id, "combat_arms:w");
    let m = w.magazine.as_ref().unwrap();
    assert_eq!(m.clip, 30);
    // 3000 world units, the built-in default (the document is in world units, 0.01 m).
    assert_eq!(w.hitscan.as_ref().unwrap().range, 3000.0);
    // RapidFireCount's default 1: semi-automatic.
    assert!(!w.trigger.as_ref().unwrap().automatic);
    // A rejected install document: ours unchanged.
    let set = merged_w(r#"[Weapon0] Name="W" ShotsPerClip=30 k=1e5"#);
    let w = &set.weapons[0];
    assert_eq!(w.magazine.as_ref().unwrap().clip, 25);
    assert_eq!(w.hitscan.as_ref().unwrap().range, 4000.0);
}

/// "Our loader": the default ammo's reserve, and install weapons with no
/// match become new documents copied from ours of the same class, after
/// ours.
#[test]
fn install_ammo_and_new_records() {
    let install = r#"
        [Ammo0] Name="A" MaxAmount=120
        [Ammo1] Name="B ammo" MaxAmount=40
        [Weapon0] Name="New Gun" Guntype=4 ShotsPerClip=10 AmmoName0="B ammo"
        [Weapon1] Name="W" Guntype=4 ShotsPerClip=30 RapidFireCount=0 Range=3500 AmmoName0="A"
        [Weapon2] Name="Pistol" Guntype=1 AmmoName0="A"
    "#;
    let set = merged_w(install);
    let ids: Vec<_> = set.weapons.iter().map(|w| w.id.as_str()).collect();
    assert_eq!(ids, ["combat_arms:w", "combat_arms:new_gun"], "the pistol has no class in our data");
    let w = &set.weapons[0];
    assert!(w.trigger.as_ref().unwrap().automatic);
    assert_eq!(w.hitscan.as_ref().unwrap().range, 3500.0);
    assert_eq!(set.ammo["combat_arms:a"].max, 90);
    let n = &set.weapons[1];
    assert_eq!(n.magazine.as_ref().unwrap().clip, 10);
    assert_eq!(n.magazine.as_ref().unwrap().ammo, "combat_arms:b_ammo");
    assert_eq!(set.ammo["combat_arms:b_ammo"].max, 30);
    assert_eq!(slug("B ammo"), "b_ammo");
}

/// "Our loader" for movement: the install's FigureType0 replaces our
/// figure.
#[test]
fn movement_takes_figure_type_0() {
    let mut data: MovementData = toml::from_str(MOVEMENT_DATA).unwrap();
    let models = ModelsFile::read(&doc(
        r#"[FigureType0] Name="Male" DashSpeed=1.2 Maxstamina=120 DashGauge=20 GaugeRec=15 VariableJump=0.1"#,
    ));
    merge_movement(&mut data, &models);
    let f = &data.character.figure;
    assert_eq!((f.dash_speed, f.max_stamina, f.dash_gauge, f.gauge_recovery, f.variable_jump), (1.2, 120.0, 20.0, 15.0, 0.1));
    // Gear isn't the models file's: unchanged.
    assert_eq!(data.character.gear.len(), 1);
}

/// A loader with no readable files gets nothing and says why per file.
#[test]
fn unreadable_files_are_reported() {
    let (attrs, problems) =
        read_attributes(|name| Err(std::io::Error::new(std::io::ErrorKind::PermissionDenied, name.to_string())));
    assert_eq!(attrs, InstallAttributes::default());
    assert_eq!(problems.len(), 3);
}
