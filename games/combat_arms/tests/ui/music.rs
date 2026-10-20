//! The front end's music and ambience (`ui::music`, headless): what the
//! client decided (`game::music`) is played as long-lived sounds
//! (`map::SoundControl`), the volume following the fade; a stream that
//! leaves the player stops; the loading screen and a window with no
//! pixels are told to the music (specs/combat_arms/music_ambience.md 2,
//! 3, 8; cases 1, 5, 18 as the front end sees them).

use std::path::PathBuf;

use bevy::{
    prelude::*,
    window::{PrimaryWindow, WindowResolution},
};
use chimera_world::{
    greybox::GreyboxMapPlugin,
    harness::Sim,
    map::{SoundControl, SoundKey},
};
use combat_arms::{
    game::music::{
        CaMusicSet, MusicClock, MusicLoading,
        ambient::SoundFx,
        data::MusicData,
        director::{CaMusic, MusicFacts, Screen},
        mix::CaVolumes,
    },
    harness::CaSim,
    ui::{
        loading::CaLoading,
        music::{CaMusicPlayPlugin, Playback, ambient_volume},
    },
};

#[derive(Resource, Default)]
struct Seen(Vec<SoundControl>);

fn collect(mut r: MessageReader<SoundControl>, mut seen: ResMut<Seen>) {
    seen.0.extend(r.read().cloned());
}

fn sim() -> Sim {
    let mut sim = Sim::with(CaSim, |app| {
        app.add_plugins((GreyboxMapPlugin, CaMusicPlayPlugin))
            .add_message::<SoundControl>()
            .init_resource::<Seen>()
            .add_systems(Update, collect.after(CaMusicSet));
    });
    sim.app.world_mut().resource_mut::<MusicClock>().manual = true;
    sim.ticks(3);
    sim
}

/// A one-second 16-bit mono WAV in a folder of ours.
fn wav(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join("ca_music_ui_test");
    std::fs::create_dir_all(&dir).unwrap();
    let samples = 8000usize;
    let mut b = Vec::new();
    b.extend(b"RIFF");
    b.extend((36 + samples as u32 * 2).to_le_bytes());
    b.extend(b"WAVEfmt ");
    b.extend(16u32.to_le_bytes());
    b.extend(1u16.to_le_bytes());
    b.extend(1u16.to_le_bytes());
    b.extend(8000u32.to_le_bytes());
    b.extend(16000u32.to_le_bytes());
    b.extend(2u16.to_le_bytes());
    b.extend(16u16.to_le_bytes());
    b.extend(b"data");
    b.extend((samples as u32 * 2).to_le_bytes());
    b.extend((0..samples).flat_map(|i| ((i as i16 % 100) * 50).to_le_bytes()));
    let path = dir.join(name);
    std::fs::write(&path, b).unwrap();
    path
}

fn set_now(sim: &mut Sim, ms: f64) {
    sim.app.world_mut().resource_mut::<MusicClock>().now = ms;
}

fn starts(sim: &Sim) -> Vec<(SoundKey, f32, bool)> {
    sim.app
        .world()
        .resource::<Seen>()
        .0
        .iter()
        .filter_map(|c| match c {
            SoundControl::Start(s) => Some((s.key, s.volume.unwrap_or(1.0), s.looping)),
            _ => None,
        })
        .collect()
}

fn changes(sim: &Sim) -> Vec<(SoundKey, f32)> {
    sim.app
        .world()
        .resource::<Seen>()
        .0
        .iter()
        .filter_map(|c| match c {
            SoundControl::Change {
                key, volume: Some(v), ..
            } => Some((*key, *v)),
            _ => None,
        })
        .collect()
}

/// The menu playlist's one track plays as a looping sound after it
/// decodes, its volume the music master times the fade; a stopped
/// player stops the sound.
#[test]
fn the_playlist_plays_and_follows_the_fade() {
    let mut sim = sim();
    let track = MusicData::built_in().playlist[0].file.clone();
    {
        let w = sim.app.world_mut();
        w.resource_mut::<Playback>().add_file(&track, wav("loop.wav"));
        let mut m = w.resource_mut::<CaMusic>();
        m.files.insert(&track, Some(1000.0));
        m.screen = Screen::Menu;
        w.resource_mut::<CaVolumes>().set_music(60);
    }
    set_now(&mut sim, 0.0);
    for _ in 0..200 {
        sim.ticks(1);
        if !starts(&sim).is_empty() {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    let s = starts(&sim);
    assert_eq!(s.len(), 1, "one stream: {:?}", sim.app.world().resource::<Seen>().0);
    assert!(s[0].2, "a single track loops");
    assert!(s[0].1.abs() < 1e-3, "the 10 s fade-in starts from silence: {}", s[0].1);
    // Half way through the fade at music 60: 0.5 x 0.6.
    set_now(&mut sim, 5_000.0);
    sim.ticks(2);
    let c = changes(&sim);
    assert!((c.last().unwrap().1 - 0.3).abs() < 1e-3, "{c:?}");
    // The music switch off: silent, the sound keeps going.
    sim.app.world_mut().resource_mut::<CaVolumes>().music_on = false;
    sim.ticks(2);
    assert_eq!(changes(&sim).last().unwrap().1, 0.0);
    assert_eq!(sim.app.world().resource::<Playback>().counts().0, 1);
    // Stopping the player stops the sound.
    sim.app.world_mut().resource_mut::<CaMusic>().player.stop();
    sim.app.world_mut().resource_mut::<CaMusic>().paused = true;
    sim.ticks(2);
    assert!(
        sim.app
            .world()
            .resource::<Seen>()
            .0
            .iter()
            .any(|c| matches!(c, SoundControl::Stop(k) if *k == s[0].0)),
        "stopped"
    );
    assert_eq!(sim.app.world().resource::<Playback>().counts().0, 0);
}

/// The loading screen is told to the music, which then plays its
/// loading track and stops the playlist (case 6).
#[test]
fn the_loading_screen_reaches_the_music() {
    let mut sim = sim();
    sim.app.world_mut().resource_mut::<CaMusic>().files =
        combat_arms::game::music::director::MusicFiles::from_data(MusicData::built_in());
    set_now(&mut sim, 0.0);
    sim.ticks(3);
    assert!(
        sim.app.world().resource::<CaMusic>().player.playing(),
        "the playlist runs at the title"
    );
    sim.app.world_mut().insert_resource(CaLoading {
        label: Some("server".into()),
        ..Default::default()
    });
    sim.ticks(3);
    assert!(sim.app.world().resource::<MusicLoading>().0);
    let facts = sim.app.world().resource::<MusicFacts>().clone();
    assert_eq!(facts.screen, Screen::Loading);
    let m = sim.app.world().resource::<CaMusic>();
    assert_eq!(
        m.player.primary.as_ref().map(|s| s.file.as_str()),
        Some("bgm/map_loading.mp3")
    );
}

/// Both masters go to 0 while the window has no pixels, back when it has
/// (case 18).
#[test]
fn a_window_without_pixels_is_minimized() {
    let mut sim = sim();
    let w = sim
        .app
        .world_mut()
        .spawn((
            Window {
                resolution: WindowResolution::new(0, 0),
                ..Default::default()
            },
            PrimaryWindow,
        ))
        .id();
    sim.app.world_mut().resource_mut::<CaVolumes>().set_music(80);
    sim.ticks(2);
    let v = *sim.app.world().resource::<CaVolumes>();
    assert!(v.minimized);
    assert_eq!((v.music(), v.sound()), (0.0, 0.0));
    sim.app.world_mut().entity_mut(w).insert(Window {
        resolution: WindowResolution::new(1280, 720),
        ..Default::default()
    });
    sim.ticks(2);
    let v = *sim.app.world().resource::<CaVolumes>();
    assert!(!v.minimized);
    assert!((v.music() - 0.8).abs() < 1e-6);
}

/// An ambient sound's played volume is its distance gain times its
/// Volume and the effects master; a placed one keeps Volume and lets the
/// radius level do the rest.
#[test]
fn ambient_volumes() {
    let mut fx = SoundFx::from_props(
        0,
        "birds",
        &combat_arms::game::objects::runtime::Props(vec![
            ("OuterRadius".into(), "6000".into()),
            ("InnerRadius".into(), "4000".into()),
            ("Volume".into(), "50".into()),
        ]),
        Vec3::ZERO,
        true,
        0,
    );
    let at = |d: f32| Some(Vec3::new(d, 0.0, 0.0));
    assert!((ambient_volume(&fx, at(3000.0), 0.8) - 0.4).abs() < 1e-5);
    assert!((ambient_volume(&fx, at(4200.0), 1.0) - 0.295).abs() < 3e-3);
    assert_eq!(ambient_volume(&fx, at(6500.0), 1.0), 0.0);
    assert_eq!(ambient_volume(&fx, None, 1.0), 0.0);
    fx.ambient = false;
    assert!((ambient_volume(&fx, at(6500.0), 0.8) - 0.4).abs() < 1e-5);
}
