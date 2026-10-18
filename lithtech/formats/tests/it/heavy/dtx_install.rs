//! The texture reader on the user's Combat Arms install, against some of
//! the test cases of specs/combat_arms/textures_dtx.md (Steam build
//! 25595979): the header and level layout cases and the whole-install
//! sweep over the unkeyed archives. Passes with a note without an install.

use std::collections::BTreeMap;

use lithtech_formats::{
    dtx::{Dtx, Format},
    rez::{RezArchive, RezKeys},
};
use source_formats::fs::FileSource;

use super::rez_install::game_dir;

fn read(archive: &str, path: &str) -> Option<Vec<u8>> {
    let rez = RezArchive::open(game_dir()?.join(archive), &RezKeys::new()).unwrap();
    Some(rez.read(path).unwrap().unwrap())
}

/// Test cases 1, 3, 4, 7, 9 and 11.
#[test]
fn layout_cases() {
    let Some(b) = read("TEXTURES.rez", "textures/metal/256xrustymetalpannel01.dtx") else { return };
    assert_eq!(b.len(), 43_844);
    let t = Dtx::parse(b).unwrap();
    assert_eq!((t.width, t.height, t.format, t.mip_count), (256, 256, Format::Dxt1, 6));
    assert_eq!((t.flags, t.surface_flags, t.group, t.use_levels), (0x8, 20, 0, 6));
    assert_eq!(t.command, "");
    assert_eq!(t.offsets, [164, 32_932, 41_124, 43_172, 43_684, 43_812, 43_844]);
    assert_eq!(t.rgba(0).len(), 256 * 256 * 4);

    let t = Dtx::parse(read("ui_hud_common.rez", "ui_hud_common/guide/guide_item_mutation.dtx").unwrap()).unwrap();
    assert_eq!((t.width, t.height, t.format, t.offsets.clone()), (64, 64, Format::Bgra32, vec![164, 16_548]));
    assert_eq!(t.rgba(0).as_chunks::<4>().0.iter().filter(|p| p[3] == 255).count(), 178);

    let t = Dtx::parse(read("PROPS.rez", "props/trees/512xcocotree01.dtx").unwrap()).unwrap();
    assert_eq!((t.width, t.height, t.format, t.surface_flags), (512, 256, Format::Dxt5, 30));
    assert_eq!(t.alpha_ref(), Some(128));
    assert_eq!(t.offsets, [164, 131_236, 164_004, 172_196, 174_244, 174_756, 174_884]);

    let t = Dtx::parse(read("PROPS.rez", "props/sand_hog/16xgaugebar01.dtx").unwrap()).unwrap();
    assert_eq!((t.width, t.height), (8, 16));
    assert_eq!(t.offsets, [164, 676, 804, 836, 844]);
    assert_eq!(t.level_dims(3), (1, 2));

    let t = Dtx::parse(read("FX.rez", "fx/cutscene/ft_zombie/cm_vst_hv_de_nx_yk.dtx").unwrap()).unwrap();
    assert_eq!((t.mip_count, t.use_levels), (8, 41));
    assert_eq!(*t.offsets.last().unwrap(), 174_924);

    let empty = read("TEXTURES.rez", "textures/door/512xwindow01.dtx").unwrap();
    assert!(empty.is_empty() && Dtx::parse(empty).is_err());

    let t = Dtx::parse(read("TEXTURES.rez", "textures/objects/gb_sprinkler.dtx").unwrap()).unwrap();
    assert_eq!((t.command.as_str(), t.alpha_ref()), ("aipharef 128", None));
}

/// Test case 14: every `.dtx` in the unkeyed archives.
#[test]
fn whole_install_sweep() {
    let Some(game) = game_dir() else { return };
    let mut paths: Vec<_> = std::fs::read_dir(&game).unwrap().flatten().map(|e| e.path()).collect();
    paths.retain(|p| p.extension().is_some_and(|x| x.eq_ignore_ascii_case("rez")));
    let (mut entries, mut empty) = (0, 0);
    let mut formats = BTreeMap::new();
    for p in paths {
        let rez = RezArchive::open(&p, &RezKeys::new()).unwrap();
        if rez.header().key_scheme().is_some() {
            continue;
        }
        for e in rez.entries().into_iter().filter(|e| e.path.ends_with(".dtx")) {
            entries += 1;
            let bytes = rez.read(&e.path).unwrap().unwrap();
            if bytes.is_empty() {
                empty += 1;
                continue;
            }
            let len = bytes.len();
            let t = Dtx::parse(bytes).unwrap_or_else(|err| panic!("{}: {err}", e.path));
            assert_eq!(*t.offsets.last().unwrap(), len, "{}", e.path);
            assert_ne!(t.flags & 0x8, 0, "{}", e.path);
            *formats.entry(format!("{:?}", t.format)).or_insert(0) += 1;
        }
    }
    assert_eq!((entries, empty), (9_658, 1));
    assert_eq!(
        formats,
        BTreeMap::from([
            ("Bgra32".to_string(), 3_237),
            ("Dxt1".to_string(), 4_068),
            ("Dxt3".to_string(), 457),
            ("Dxt5".to_string(), 1_895)
        ])
    );
}
