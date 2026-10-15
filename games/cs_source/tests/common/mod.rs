//! What both of `cs_source`'s test binaries share (`mod common;` in
//! tests/it/main.rs and tests/ui/main.rs; a change here runs all of the
//! crate's tests, `repo_checks`' `affected`).
//!
//! `load_map`: a CS:S map by id from the user's install, as the CS:S app
//! loads it (`cs_source_app::games::load_map_level`, which tests here
//! can't use: the app sits above this crate).

use chimera_core::mount::config::LocalConfig;
use chimera_world::map::MapData;

/// Map `id` (`cs_source:<name>`) from the install (mashup.local.toml,
/// saved settings or Steam).
#[allow(dead_code)]
pub fn load_map(id: &str) -> Result<MapData, String> {
    load_map_level(id, 0)
}

/// `load_map` at a Source `mat_hdr_level` (0 LDR, 1 bloom, 2 HDR).
pub fn load_map_level(id: &str, hdr_level: u8) -> Result<MapData, String> {
    let name = id
        .strip_prefix("cs_source:")
        .ok_or_else(|| format!("map ID `{id}` should look like cs_source:name"))?;
    let install = LocalConfig::load()?
        .game_path(cs_source::game::GAME)
        .ok_or("no `cs_source` install found (mashup_install <folder>, or mashup.local.toml)")?;
    let mount = cs_source::game::mount::open(&install).map_err(|e| e.to_string())?;
    cs_source::game::bsp::load_level(&mount, name, hdr_level)
}
