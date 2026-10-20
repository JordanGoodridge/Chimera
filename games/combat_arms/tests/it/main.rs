//! `combat_arms`' integration tests, as one test binary (each file a
//! module): scenario tests of Combat Arms' simulation on the engine's
//! headless harness (`chimera_world::harness::Sim`) with its base
//! (`combat_arms::harness::CaSim`) and the greybox, with nothing above
//! the game. `cargo test -p combat_arms --features dev --test it`; where
//! tests go: docs/OBSERVABILITY.md, section 1.
//!
//! Wave 0 checks the skeleton: a player spawns, walks, fires the
//! placeholder rifle and switches to the placeholder knife. Spec test
//! cases (specs/combat_arms/) get their own files here as each subsystem
//! is implemented.

mod aim_points;
mod attachments;
mod attribute_data;
mod bot_objectives;
mod bots;
mod capture_the_flag;
mod characters;
mod clan_modes;
mod death_killcam;
mod effects_sounds;
mod free_for_all;
mod gear_perks;
mod game_modes;
mod grenades;
mod hired_guns;
mod inventory;
mod mode_kit;
mod model;
mod movement;
mod movement_corrections;
mod movement_volumes;
mod net;
mod net_modes;
mod net_modes_2;
mod radio_chat;
mod radio_chat_net;
mod other_modes;
mod radar;
mod search_destroy;
mod skeleton;
mod special_devices;
mod special_weapons;
mod view_model;
mod vip_escort;
mod weapon_classes;
mod weapon_corrections;
mod weapon_firing;
mod world;
mod world_objects;
mod world_objects_play;

// Slow ones (the user's install): the fast tier skips `heavy::`.
mod heavy {
    mod attributes;
    mod bots;
    mod characters;
    mod effects_sounds;
    mod junk_flea2;
    mod map_load;
    mod modes_part_two;
    mod model;
    mod movement_volumes;
    mod mount;
    mod props;
    mod radio;
    mod radar;
    mod view_model;
    mod world;
    mod world_objects;
}
