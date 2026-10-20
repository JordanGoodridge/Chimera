//! The per-map image data in the mission files (specs/combat_arms/
//! radar_minimap.md R1): in each `[ObjectN]` section the image's archive
//! name (`LargeMap`), its size (`LargeMapWidth`, `LargeMapHeight`) and two
//! pixel to editor-space pairs (`LargeMapPoint0_2D` / `_3D`, `Point1`),
//! from which the corners follow (`maths::corners`). A file without the
//! fields gives no image and NaN corners; the radar still shows icons.
//!
//! A mission file is picked by its world and mode (`find`): the files
//! `gms/<map>_<suffix>.gms` whose `[MapInfo]` `Map` names the world, the
//! suffix by the mode's room key (missions.md, "Where the data lives").
//! That is this module's reading of the mission files until the mission
//! reader (missions.md) is in; it reads only the map fields.

use lithtech_formats::attributes::{Document, Section};
use serde::{Deserialize, Serialize};

use super::maths::{Corners, corners};
use crate::mount::Mount;

/// What one objective says of the map image.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct MapImage {
    /// The image's archive name as written (`LargeMap`); empty: none.
    pub name: String,
    pub size: (i32, i32),
    /// The corners; None when the data is missing (NaN).
    pub corners: Option<Corners>,
}

impl Default for MapImage {
    fn default() -> Self {
        Self {
            name: String::new(),
            size: (0, 0),
            corners: None,
        }
    }
}

impl MapImage {
    /// The fields of one objective section (R1), with the spec's defaults
    /// (0 for sizes and points).
    pub fn read(s: &Section) -> Self {
        let name = s.fixed_string("LargeMap", "");
        let (w, h) = (s.int("LargeMapWidth", 0), s.int("LargeMapHeight", 0));
        let point = |k: &str| s.point(k).map_or((0, 0), |p| (p[0], p[1]));
        let world = |k: &str| s.vector(k).map_or((0.0, 0.0), |v| (v[0], v[2]));
        let c = corners(
            point("LargeMapPoint0_2D"),
            world("LargeMapPoint0_3D"),
            point("LargeMapPoint1_2D"),
            world("LargeMapPoint1_3D"),
            (w, h),
        );
        Self {
            name,
            size: (w, h),
            corners: c.is_finite().then_some(c),
        }
    }

    /// Objective `n` of a mission file (the versus modes play 0).
    pub fn of_document(doc: &Document, n: usize) -> Self {
        doc.section(&format!("Object{n}")).map(Self::read).unwrap_or_default()
    }

    /// The image's texture key in the mount (`ui_hud_map/mapimage/...`).
    pub fn key(&self) -> Option<String> {
        (!self.name.trim().is_empty()).then(|| super::look::map_image_key(&self.name))
    }
}

/// The mission file suffix of a room key (`GMS\<MAP>_<SUFFIX>.GMS`;
/// missions.md's table).
pub fn suffix(mode_key: &str) -> Option<&'static str> {
    Some(match mode_key {
        "TDM" => "t",
        "TSV" => "tsv",
        "TMM" => "m",
        "CTF" => "ctf",
        "FFA" => "f",
        "LMS" => "lms",
        "BNT" => "bnt",
        "QRT" => "qrt",
        "SBT" => "sbt",
        "HQM" => "hqm",
        "VTDM" => "vtdm",
        "ESC" => "esc",
        "CESC" => "cesc",
        "VWRM" => "vwrm",
        "HG" => "hg",
        "ESL" => "esl",
        "CDM" => "ct",
        "CMM" => "cm",
        "CCTF" => "cctf",
        "CTSV" => "ctsv",
        _ => return None,
    })
}

/// A world file name as the mission's `Map` field writes it, as the map
/// name (`Worlds2\Warehouse.dat` -> `warehouse`).
pub fn world_stem(map_field: &str) -> String {
    let n = map_field.trim().replace('\\', "/").to_lowercase();
    let file = n.rsplit('/').next().unwrap_or("");
    file.strip_suffix(".dat").unwrap_or(file).to_string()
}

/// The mission file for world `map` (`warehouse`) in mode `mode_key`:
/// its path and the parsed document. `None` when the install has no such
/// file (or its archive is locked).
pub fn find(mount: &Mount, map: &str, mode_key: &str) -> Option<(String, Document)> {
    let want = world_stem(map);
    let tail = format!("_{}.gms", suffix(mode_key)?);
    for path in mount.files_in("gms") {
        let file = path.rsplit('/').next().unwrap_or("");
        if !file.ends_with(&tail) || file.len() == tail.len() {
            continue;
        }
        let Ok(bytes) = mount.read(&path) else { continue };
        let Ok(doc) = Document::parse(&bytes) else { continue };
        let named = doc.section("MapInfo").and_then(|s| s.string("Map")).map(|m| world_stem(&String::from_utf8_lossy(m)));
        if named.as_deref() == Some(want.as_str()) {
            return Some((path, doc));
        }
    }
    None
}

/// The map image data of world `map` in mode `mode_key`, objective 0.
pub fn image_of(mount: &Mount, map: &str, mode_key: &str) -> Option<MapImage> {
    find(mount, map, mode_key).map(|(_, doc)| MapImage::of_document(&doc, 0))
}

#[cfg(test)]
mod tests {
    use super::*;

    const SHOWDOWN: &str = "[Object0]\nLargeMap = \"UI_HUD_Map\\mapImage\\Map_showdown.dtx\"\nLargeMapWidth = 1024\nLargeMapHeight = 1024\nLargeMapPoint0_2D = (128, 128)\nLargeMapPoint0_3D = <-2048, 0, 2304>\nLargeMapPoint1_2D = (895, 895)\nLargeMapPoint1_3D = <1536, 0, -1280>\n";

    #[test]
    fn corners_of_a_section() {
        let doc = Document::parse(SHOWDOWN.as_bytes()).unwrap();
        let m = MapImage::of_document(&doc, 0);
        let c = m.corners.unwrap();
        assert!((c.lt.0 - -2646.112).abs() < 0.01 && (c.lt.1 - 2902.112).abs() < 0.01, "{c:?}");
        assert_eq!(m.key().as_deref(), Some("ui_hud_map/mapimage/map_showdown.dtx"));
    }

    #[test]
    fn a_missing_section_has_no_image() {
        let doc = Document::parse(b"[Object0]\nTimeLimit = 300\n").unwrap();
        let m = MapImage::of_document(&doc, 0);
        assert_eq!((m.name.as_str(), m.corners), ("", None));
        assert_eq!(m.key(), None);
    }
}
