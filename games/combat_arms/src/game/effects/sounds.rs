//! Playing Combat Arms' sounds (specs/combat_arms/effects_sounds.md 10):
//! every effect writes a `CaSound` (the archive file, local or 3D at a
//! point with inner and outer radius, volume 0..100); `play` turns it
//! into Chimera's `PlaySound` with a `SoundLevel::Radius` (the
//! squared-distance gain and the distance low-pass, `rules`), loading the
//! file from the install the first time (`library::sound_clip`) and
//! adding it to the map's `SoundBank`. Server sounds (hurt, grenade
//! bounces) go out as `GameSound`, which a network server also sends to
//! every client.
//!
//! Known shortcuts (docs/tech-debt.md): a 3D sound's gain and low-pass
//! are set when it starts (Chimera's one-shots don't follow a moving
//! listener); server sounds reach every client, not those within 2R
//! (`rules::server_sends`: beyond R they are silent anyway); priorities
//! aren't kept (none are used to pick) and Chimera plays every sound
//! started (no 255-voice cap).

use std::collections::HashMap;

use bevy::prelude::*;

use super::{library, rules};
use crate::map::{
    GameSound, MapSoundEntry, PlaySound, SoundLevel,
    sound::{Interval, SoundBank},
};

/// A sound to play.
#[derive(Message, Clone, Debug, PartialEq)]
pub struct CaSound {
    /// The file inside the install's archives.
    pub file: String,
    /// Where (metres); None: local (2D, in your head).
    pub at: Option<Vec3>,
    /// Inner and outer radius (u) of a 3D sound.
    pub radii: (f32, f32),
    /// 0..100.
    pub volume: f32,
    /// Who makes it, and on which channel: a new sound replaces the one
    /// that source plays there.
    pub source: Option<Entity>,
    pub channel: Option<u8>,
    /// A server sound (sent to clients).
    pub server: bool,
}

impl CaSound {
    /// A local sound (yours: fire, reload, footsteps, dings, interface).
    pub fn local(file: impl Into<String>) -> Self {
        Self {
            file: file.into(),
            at: None,
            radii: (0.0, 0.0),
            volume: 100.0,
            source: None,
            channel: None,
            server: false,
        }
    }

    /// A 3D sound at `at` (metres) with outer radius R (u) and the inner
    /// radius 7 % of it.
    pub fn at(file: impl Into<String>, at: Vec3, big_r: f32) -> Self {
        Self {
            at: Some(at),
            radii: (rules::inner_radius(big_r), big_r),
            ..Self::local(file)
        }
    }

    pub fn volume(self, volume: f32) -> Self {
        Self { volume, ..self }
    }

    pub fn from_source(self, source: Entity, channel: u8) -> Self {
        Self {
            source: Some(source),
            channel: Some(channel),
            ..self
        }
    }

    pub fn server(self) -> Self {
        Self { server: true, ..self }
    }

    /// The bank entry name it plays as.
    pub fn entry(&self) -> String {
        match self.at {
            None => format!("ca:{}", self.file),
            Some(_) => format!("ca:{}@{}:{}", self.file, self.radii.0, self.radii.1),
        }
    }
}

/// Sound channels by source.
pub const CHANNEL_FIRE: u8 = 1;
pub const CHANNEL_GRENADE: u8 = 2;

/// The install's sound files loaded so far: clip index in the bank (None:
/// doesn't read).
#[derive(Resource, Default)]
pub struct CaSoundFiles(pub HashMap<String, Option<usize>>);

/// Play the frame's `CaSound`s.
pub fn play(
    mut sounds: MessageReader<CaSound>,
    bank: Option<ResMut<SoundBank>>,
    mut files: ResMut<CaSoundFiles>,
    mut lengths: ResMut<super::CaSoundLengths>,
    mut out: MessageWriter<PlaySound>,
    mut game: MessageWriter<GameSound>,
) {
    let Some(mut bank) = bank else {
        sounds.clear();
        return;
    };
    let mount = library::install_mount();
    if bank.is_added() {
        files.0.clear();
        // Server sounds arrive by entry name: every client has them ready
        // (their radii as the server sends them, `rules::server_radii`).
        if let Some(mount) = mount.as_deref() {
            for s in server_sounds(super::data::CaFxTables::built_in()) {
                ensure(&mut bank, &mut files, &mut lengths, mount, &s);
            }
        }
    }
    for s in sounds.read() {
        let entry = s.entry();
        if bank.0.entry(&entry).is_none() {
            let Some(mount) = mount.as_deref() else { continue };
            if !ensure(&mut bank, &mut files, &mut lengths, mount, s) {
                continue;
            }
        }
        let play = PlaySound {
            entry,
            at: s.at,
            volume: Some((s.volume / 100.0).clamp(0.0, 1.0)),
            pitch: None,
            source: s.source,
            channel: s.channel,
        };
        if s.server {
            game.write(GameSound(play));
        } else {
            out.write(play);
        }
    }
}

/// Make `s`'s bank entry, reading its file the first time (false: it
/// doesn't read).
fn ensure(
    bank: &mut SoundBank,
    files: &mut CaSoundFiles,
    lengths: &mut super::CaSoundLengths,
    mount: &crate::mount::Mount,
    s: &CaSound,
) -> bool {
    let entry = s.entry().to_lowercase();
    if bank.0.entry(&entry).is_some() {
        return true;
    }
    let clip = *files.0.entry(s.file.clone()).or_insert_with(|| {
        let clip = library::sound_clip(&s.file, mount)?;
        let frames = clip.samples.len() / clip.channels.max(1) as usize;
        lengths.0.insert(s.file.clone(), frames as f32 / clip.rate.max(1) as f32);
        let sounds = std::sync::Arc::make_mut(&mut bank.0);
        sounds.clips.push(clip);
        Some(sounds.clips.len() - 1)
    });
    let Some(clip) = clip else { return false };
    let level = match s.at {
        Some(_) => rules::sound_level_radii(s.radii.0, s.radii.1, super::super::inventory::UNIT),
        None => SoundLevel::Db(0.0),
    };
    std::sync::Arc::make_mut(&mut bank.0).entries.insert(
        entry,
        MapSoundEntry {
            waves: vec![clip],
            volume: Interval::fixed(1.0),
            pitch: Interval::fixed(100.0),
            level,
            channel: 0,
            dry: false,
        },
    );
    true
}

/// The server sounds the tables name, as clients get them (radii
/// quantised): hurt sounds and grenade bounces.
pub fn server_sounds(t: &super::data::CaFxTables) -> Vec<CaSound> {
    let mut out = Vec::new();
    let s = &t.sounds;
    let hurt = rules::server_radii(rules::inner_radius(s.hurt_radius), s.hurt_radius);
    for f in [&s.guns, &s.explosions, &s.burning, &s.falling, &s.gas, &s.head, &s.groin]
        .into_iter()
        .flatten()
    {
        out.push(server_sound(f, Vec3::ZERO, hurt));
    }
    for surface in &t.surface {
        if !surface.grenade.is_empty() {
            let r = rules::server_radii(rules::inner_radius(surface.grenade_radius), surface.grenade_radius);
            out.push(server_sound(&surface.grenade, Vec3::ZERO, r));
        }
    }
    out
}

/// A server sound at `at` with radii `(r, R)` as clients get them.
pub fn server_sound(file: &str, at: Vec3, (r, big_r): (f32, f32)) -> CaSound {
    CaSound {
        radii: (r, big_r),
        ..CaSound::at(file, at, big_r).server()
    }
}
