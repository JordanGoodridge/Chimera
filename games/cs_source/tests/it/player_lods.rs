//! The CS:S player models' levels of detail, read from the install
//! (skipped without one): each ends in a level with switch point -1 (a
//! copy of the level before it, for shadows) that is never drawn by
//! distance, so a body draws LOD 0 up close and a lower level far off.

use ::cs_source::game::{self as cs_source, mount, props};
use bevy::math::Vec3;
use chimera_core::mount::config::LocalConfig;
use chimera_present::render::lod::{ModelSettings, metric, pick};

fn read(path: &str) -> Option<Vec<u8>> {
    let config = LocalConfig::load().ok()?;
    let m = mount::open(&config.game_path_in(cs_source::GAME, cs_source::INFO.steam.as_ref())?).ok()?;
    m.read(path).ok()
}

#[test]
fn player_bodies_draw_lod_0_up_close_and_never_the_shadow_level() {
    for (model, drawn) in [
        ("ct_urban", [10.0, 20.0, 35.0, 50.0, 70.0]),
        ("ct_sas", [5.0, 15.0, 30.0, 50.0, 70.0]),
        ("t_leet", [10.0, 20.0, 35.0, 50.0, 70.0]),
        ("t_phoenix", [10.0, 20.0, 35.0, 50.0, 70.0]),
    ] {
        let stem = format!("models/player/{model}");
        let (Some(vtx), Some(vvd)) = (read(&format!("{stem}.dx90.vtx")), read(&format!("{stem}.vvd"))) else {
            return;
        };
        let vtx = vmdl::vtx::Vtx::read(&vtx).expect("a .vtx");
        let vvd = vmdl::vvd::Vvd::read(&vvd).expect("a .vvd");
        // The file's levels: LOD 0, the five above, then -1.
        let file: Vec<f32> = vtx.body_parts[0].models[0].lods.iter().map(|l| l.switch_point).collect();
        assert_eq!(file.last(), Some(&-1.0), "{model}: {file:?}");
        let switches = props::lod_switches(&vtx);
        assert_eq!(switches, drawn, "{model}: the levels drawn by distance");
        // The bind pose's bounding sphere (Source units), as a body's
        // `LodGroup` takes it, seen at fov 90 (4:3).
        let (lo, hi) = vvd.vertices.iter().fold((Vec3::MAX, Vec3::MIN), |(lo, hi), v| {
            let p = Vec3::new(v.position.x, v.position.y, v.position.z);
            (lo.min(p), hi.max(p))
        });
        let radius = (hi - lo).length() / 2.0;
        assert!((35.0..60.0).contains(&radius), "{model}: radius {radius}");
        let levels: Vec<f32> = std::iter::once(0.0).chain(switches).collect();
        let at = |m: f32| pick(&levels, metric(m / 0.0254, radius, 0.75), &ModelSettings::default());
        eprintln!(
            "{model}: levels {file:?}, radius {radius:.1}, at 2/10/40 m: {} {} {}",
            at(2.0),
            at(10.0),
            at(40.0)
        );
        assert_eq!(at(2.0), 0, "{model} at 2 m");
        assert!(at(10.0) <= 1, "{model} at 10 m: {}", at(10.0));
        assert_eq!(at(40.0), 2, "{model} at 40 m");
        assert_eq!(at(1000.0), 5, "{model} far off: its last drawn level");
    }
}
