//! Counter-Strike: Source (Steam app 240).

pub mod ambient;
pub mod anim;
pub mod beams;
pub mod breakables;
pub mod bsp;
pub mod decals;
pub mod detail;
pub mod dust;
pub mod fire;
pub mod gameui;
pub mod grenades;
pub mod hostage_anim;
pub mod hud;
pub mod impact_effects;
pub mod impacts;
pub mod lightmap;
pub mod material;
pub mod mount;
pub mod movement;
pub mod nav;
pub mod objectives;
pub mod overlays;
pub mod pain;
pub mod pcf;
pub mod player_anim;
pub mod propdata;
pub mod props;
pub mod pushaway;
pub mod radio;
pub mod ropes;
pub mod scrapes;
pub mod shadow;
pub mod sky;
pub mod sound;
pub mod soundscape;
pub mod sprites;
pub mod steam;
pub mod trails;
pub mod surfaceprops;
pub mod view_anim;
pub mod vgui;
pub mod view_motion;
pub mod water;
pub mod weapons;

// Pure-data readers moved to `source_formats`, at their old paths.
pub use source_formats::{lumps, pak, phy, vhv, vpk, wav};

pub const GAME: &str = "cs_source";

/// The map cycle a server plays in turn (`cfg/mapcycle.txt`, else the
/// install's `cfg/mapcycle_default.txt`, else an old `mapcycle.txt`): its
/// map names in order, those the install has.
pub fn map_cycle(mount: &crate::mount::Mount) -> Vec<String> {
    let maps: std::collections::HashSet<String> = mount
        .entries()
        .into_iter()
        .filter_map(|(e, _)| e.path.strip_prefix("maps/")?.strip_suffix(".bsp").map(str::to_lowercase))
        .collect();
    let has_map = |m: &String| maps.contains(m);
    ["cfg/mapcycle.txt", "cfg/mapcycle_default.txt", "mapcycle.txt"]
        .iter()
        .find_map(|p| mount.read(p).ok())
        .map(|b| parse_map_cycle(&String::from_utf8_lossy(&b)))
        .unwrap_or_default()
        .into_iter()
        .filter(has_map)
        .collect()
}

/// A map cycle file's map names: `//` comments and blank lines left out.
pub fn parse_map_cycle(text: &str) -> Vec<String> {
    text.lines()
        .map(|l| l.split("//").next().unwrap_or_default().trim())
        .filter_map(|l| l.split_whitespace().next().map(str::to_lowercase))
        .collect()
}

#[cfg(test)]
mod cycle_tests {
    #[test]
    fn map_cycle_files_read_as_names() {
        let text = "// Default mapcycle file\r\n//\r\n\r\ncs_italy\r\nde_dust // comment\n De_Aztec \n";
        assert_eq!(super::parse_map_cycle(text), ["cs_italy", "de_dust", "de_aztec"]);
    }
}

/// CS:S's server tick: 0.015 s (66.67 Hz). Measured with the movement probe
/// (tools/css_probe): a dedicated server started with -tickrate 64 still
/// accelerates and applies friction in 0.015 s steps, so CS:S fixes it.
pub const TICK_INTERVAL: f64 = 0.015;
