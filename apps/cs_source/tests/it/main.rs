//! `cs_source_app`'s integration tests, as one test binary (each file a
//! module, so the game links once for all of them: docs/performance.md,
//! "Test cycle"): scenario tests on the app's headless harness
//! (`harness::Sim`, `NetSim`: the simulation as this app composes it,
//! `SimPlugins`) and the greybox. Run one file's tests with `cargo test
//! -p cs_source_app --features dev --test it weapons::` (a name filter).
//!
//! A test goes in the lowest crate whose public API it needs
//! (docs/OBSERVABILITY.md, section 1): here when it needs the harness, the
//! greybox or `games::load_map`. A new file here (or in `heavy/`) gets a
//! `mod` line below; `architecture::tests_are_one_binary_per_crate` fails
//! for a file without one.
//!
//! `heavy` holds the slow ones (real maps from the install, long bot
//! simulations): the fast tier skips them with `-- --skip heavy::`.

mod auto_switch;
mod bot_buying;
mod bot_grenades;
mod bot_manners;
mod console_commands;
mod cs_grenades;
mod cs_guns;
mod debug_ui;
mod drawn_parts;
mod hit_reaction;
mod impact_effects;
mod interpolation;
mod join_flow;
mod logic_physics;
mod map_game_entities;
mod map_logic;
mod map_sound_fx;
mod metrics;
mod movement;
mod net;
mod net_bots;
mod net_interp;
mod net_maps;
mod net_players;
mod net_prediction;
mod net_query;
mod net_rules;
mod net_upnp;
mod net_weapons;
mod night_vision;
mod objectives;
mod pain_sounds;
mod prediction;
mod prop_lods;
mod prop_shadows;
mod radio;
mod ragdoll;
mod rounds;
mod source_movement;
mod source_movement_css;
mod source_movement_ladder_water;
mod source_movement_tricks;
mod spectate;
mod sprays;
mod target_id;
mod teams;
mod view_models;
mod weapon_select;
mod weapons;

/// Slow tests: the fast tier (`-- --skip heavy::`) leaves them out.
mod heavy {
    mod bot_nav;
    mod bot_radio;
    mod bot_rounds;
            mod map_ambient;
    mod map_areaportals;
    mod map_breakables;
    mod map_brush_decals;
    mod map_brush_entities;
    mod map_community;
    mod map_de_dust2;
    mod map_de_nuke;
    mod map_de_nuke_doors;
    mod map_de_port;
    mod map_net_changelevel;
    mod net_big_map;
    mod net_capacity;
    mod map_net_weapons;
    mod net_soak;
    mod map_fire;
    mod map_courses;
    mod map_flows;
        mod map_ladders;
    mod map_loose;
    mod map_objectives;
    mod map_occluders;
    mod map_physics_shadow;
    mod map_prop_damage;
    mod map_prop_settle;
    mod map_props;
    mod map_sound_stock;
    mod map_spawn_dump;
        mod map_surf;
        mod map_visuals;
}
