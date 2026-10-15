//! CS:S's bomb and hostages (specs/cs_source/objectives.md): the C4 as a
//! weapon (its data: games/cs_source/data/weapons/c4.toml; `c4_part`),
//! its and the kit's models, the sound entries, the defusal kit's
//! price, the hostage models. The rules themselves are game-neutral
//! (`crate::objectives`); this fills in CS:S's data.

use bevy::prelude::*;

use crate::objectives::{
    bomb::{BombRules, BombSounds, C4 as C4Part},
    hostages::{HostageRules, HostageSounds},
};

pub const C4: &str = "cs_source:weapon_c4";
/// Held-model keys of the planted bomb and a dropped defusal kit.
pub const C4_PLANTED: &str = "cs_source:c4_planted";
pub const DEFUSER: &str = "cs_source:item_defuser";

/// World models other than the C4's own (its data's): planted, the kit.
pub const WORLD_MODELS: &[(&str, &str)] = &[
    (C4_PLANTED, "models/weapons/w_c4_planted.mdl"),
    (DEFUSER, "models/weapons/w_defuser.mdl"),
];

/// The only hostage models CS:S ships (spec Q9).
pub const HOSTAGE_MODELS: &[&str] = &[
    "models/characters/hostage_01.mdl",
    "models/characters/hostage_02.mdl",
    "models/characters/hostage_03.mdl",
    "models/characters/hostage_04.mdl",
];

/// Sound entries (spec Sounds), for precaching with the map's sounds.
pub const SOUNDS: &[&str] = &[
    "c4.click",
    "c4.plant",
    "C4.PlantSound",
    "c4.disarmstart",
    "c4.disarmfinish",
    "c4.explode",
    "defuser.equip",
    "Event.BombPlanted",
    "Event.BombDefused",
    "Event.HostageTouched",
    "Event.HostageRescued",
    "Event.HostageKilled",
    "Hostage.Pain",
    "Hostage.StartFollowCT",
    "Hostage.StopFollowCT",
];

/// The defusal kit's price (localization "DEFUSAL KIT: $200"; CTs only).
pub const DEFUSER_PRICE: u32 = 200;

pub struct CsObjectivesPlugin;

impl Plugin for CsObjectivesPlugin {
    fn build(&self, app: &mut App) {
        let s = |x: &str| Some(x.to_string());
        let mut bomb = app.world_mut().get_resource_or_init::<BombRules>();
        bomb.weapon = Some(C4);
        bomb.sounds = BombSounds {
            click: s("c4.click"),
            plant: s("c4.plant"),
            beep: s("C4.PlantSound"),
            defuse_start: s("c4.disarmstart"),
            defuse_finish: s("c4.disarmfinish"),
            explode: s("c4.explode"),
            kit: s("defuser.equip"),
            announce_planted: s("Event.BombPlanted"),
            announce_defused: s("Event.BombDefused"),
        };
        bomb.planted_model = s(C4_PLANTED);
        bomb.kit_model = s(DEFUSER);
        // w_c4_planted stands on end in the held-model frame: lay it down,
        // keypad up.
        bomb.planted_turn =
            Quat::from_rotation_z(-std::f32::consts::FRAC_PI_2) * Quat::from_rotation_x(-std::f32::consts::FRAC_PI_2);
        let mut hostages = app.world_mut().get_resource_or_init::<HostageRules>();
        hostages.models = HOSTAGE_MODELS.iter().map(|m| m.to_string()).collect();
        hostages.sounds = HostageSounds {
            pain: s("Hostage.Pain"),
            start_follow: s("Hostage.StartFollowCT"),
            stop_follow: s("Hostage.StopFollowCT"),
            announce_touched: s("Event.HostageTouched"),
            announce_rescued: s("Event.HostageRescued"),
            announce_killed: s("Event.HostageKilled"),
        };
    }
}

/// The `cs_source:c4` hook: the weapon is the bomb (the bomb rules arm
/// and plant it). Its slot, speed and pickup team are its data.
pub fn c4_part(e: &mut EntityWorldMut) {
    e.insert(C4Part);
}
