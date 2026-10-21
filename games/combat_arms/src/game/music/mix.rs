//! The two volume channels (music_ambience.md 8): music (`musicvolume`,
//! `enablemusic`) and effects (`soundvolume`, `enablesound`), 0..100 in
//! steps of 10, and the window being minimized, which sends both masters
//! to 0 until it is restored.
//!
//! Gain of a music stream is `m x slot gain`; of an effect `s x g(d) x V /
//! 100` (a 3D or ambient sound), `s x V / 100` (a 2D one with a volume),
//! `s` (an interface sound).

use bevy::prelude::*;

/// The player's sound settings and the window's state.
#[derive(Resource, Clone, Copy, Debug, PartialEq, Eq)]
pub struct CaVolumes {
    pub music_on: bool,
    /// 0..100.
    pub music_volume: u32,
    pub sound_on: bool,
    pub sound_volume: u32,
    /// The window is iconic.
    pub minimized: bool,
}

impl Default for CaVolumes {
    fn default() -> Self {
        let d = super::data::MusicData::built_in();
        Self {
            music_on: true,
            music_volume: d.music_volume_default,
            sound_on: true,
            sound_volume: d.sound_volume_default,
            minimized: false,
        }
    }
}

/// A slider's value snapped to its step and range.
pub fn snap(v: u32, step: u32) -> u32 {
    let step = step.max(1);
    ((v + step / 2) / step * step).min(100)
}

impl CaVolumes {
    /// The music master `m`: 0 when off or minimized.
    pub fn music(&self) -> f32 {
        if self.music_on && !self.minimized {
            self.music_volume.min(100) as f32 / 100.0
        } else {
            0.0
        }
    }

    /// The effects master `s`.
    pub fn sound(&self) -> f32 {
        if self.sound_on && !self.minimized {
            self.sound_volume.min(100) as f32 / 100.0
        } else {
            0.0
        }
    }

    /// The music slider (snapped to its step).
    pub fn set_music(&mut self, v: u32) {
        self.music_volume = snap(v, super::data::MusicData::built_in().slider_step);
    }

    pub fn set_sound(&mut self, v: u32) {
        self.sound_volume = snap(v, super::data::MusicData::built_in().slider_step);
    }
}

/// The gain of an effect: `s` times the distance gain `g` times the
/// sound's own volume `v` (0..100).
pub fn effect_gain(s: f32, g: f32, v: f32) -> f32 {
    s * g * v / 100.0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn masters() {
        let mut v = CaVolumes::default();
        v.music_volume = 80;
        v.sound_volume = 70;
        assert_eq!((v.music(), v.sound()), (0.8, 0.7));
        v.minimized = true;
        assert_eq!((v.music(), v.sound()), (0.0, 0.0));
        v.minimized = false;
        assert_eq!((v.music(), v.sound()), (0.8, 0.7));
        v.music_on = false;
        assert_eq!(v.music(), 0.0);
        assert_eq!(snap(64, 10), 60);
        assert_eq!(snap(250, 10), 100);
    }
}
