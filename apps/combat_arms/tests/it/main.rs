//! The Combat Arms app's integration tests: the simulation as the binary
//! composes it (`combat_arms_app::SimPlugin`) on the headless harness,
//! with maps loaded as `--map` loads them (`combat_arms_app::load_map`).

mod heavy {
    mod map_start;
}
