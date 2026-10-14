//! Sounds for any game (docs/plans/active/sound.md), the simulation's
//! side: the map carries named sound entries and decoded clips (and
//! soundscapes); game code writes `PlaySound`, and starts, changes and
//! stops long-lived sounds with `SoundControl` under a `SoundKey`; the
//! distance model (`distance_gain`, also for bots' hearing) and the
//! surface under a point (`SurfaceGrid`, footsteps). Playback is
//! `crate::audio`.

use std::{collections::HashMap, sync::Arc};

use bevy::prelude::*;

/// Decoded audio: interleaved 16-bit samples (what the format readers
/// decode to).
pub use source_formats::wav::SoundClip as MapSoundClip;

/// How a sound falls off with distance.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum SoundLevel {
    /// Source sound level (0..255 "dB"); 0 is heard everywhere.
    Db(f32),
    /// Linear falloff, silent beyond 1000 / attenuation units (GoldSrc
    /// style, Source's "CompatibilityAttenuation").
    Attenuation(f32),
}

/// A value drawn uniformly from start..start + range.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Interval {
    pub start: f32,
    pub range: f32,
}

impl Interval {
    pub const fn fixed(v: f32) -> Self {
        Self { start: v, range: 0.0 }
    }
    pub fn draw(&self, unit: f32) -> f32 {
        self.start + self.range * unit
    }
}

/// A named sound (Source: a sound script entry).
#[derive(Clone, Debug)]
pub struct MapSoundEntry {
    /// Indices into `MapData::sound_clips`; one is picked at random.
    pub waves: Vec<usize>,
    pub volume: Interval,
    /// Percent of the recorded rate.
    pub pitch: Interval,
    pub level: SoundLevel,
    pub channel: u8,
    /// Bypasses the room DSP (Source: every wave marked `#`).
    pub dry: bool,
}

/// Ground properties that sounds use (Source surface properties).
#[derive(Clone, Debug, Default)]
pub struct MapSurface {
    pub step_left: Option<String>,
    pub step_right: Option<String>,
    /// Source game material letter ('C' concrete, 'D' dirt, ...).
    pub game_material: char,
    pub bullet_impact: Option<String>,
    pub impact_soft: Option<String>,
    pub impact_hard: Option<String>,
    /// How hard it sounds when hit (audiohardnessfactor).
    pub hardness: f32,
    /// Hitting something softer than this plays the soft impact.
    pub hard_threshold: f32,
    /// Impacts slower than this play the soft sound (0: no rule), u/s.
    pub hard_min_velocity: f32,
    /// Looping friction sounds while sliding (scraperough, scrapesmooth).
    pub scrape_rough: Option<String>,
    pub scrape_smooth: Option<String>,
    /// How rough it sounds to what slides on it (audioroughnessfactor).
    pub roughness: f32,
    /// Sliding on something smoother than this plays the smooth scrape
    /// (scrapeRoughThreshold).
    pub rough_threshold: f32,
}

/// Sound data the map carries.
#[derive(Clone, Debug, Default)]
pub struct MapSounds {
    /// Lower-case entry names.
    pub entries: HashMap<String, MapSoundEntry>,
    pub clips: Vec<MapSoundClip>,
    /// Lower-case surface names.
    pub surfaces: HashMap<String, MapSurface>,
    /// Ambience: named soundscapes and where each one applies.
    pub soundscapes: Vec<Soundscape>,
    pub soundscape_zones: Vec<SoundscapeZone>,
    pub soundscape_emitters: Vec<SoundscapeEmitter>,
}

/// A map's background ambience (Source soundscapes): loops that fade in
/// while it applies, and one-shots at random intervals.
#[derive(Clone, Debug, Default)]
pub struct Soundscape {
    pub name: String,
    /// The room preset it sets (`room::PRESETS`, Source's "dsp"); None
    /// leaves the room as it was.
    pub dsp: Option<u16>,
    /// The listener's own DSP preset it sets ("dsp_player"); kept and
    /// shown, not played (`room::RoomDsp::player`).
    pub dsp_player: Option<u16>,
    /// The room's level it sets ("dsp_volume"); None puts the user's
    /// back.
    pub dsp_volume: Option<f32>,
    /// The sound mixer it names ("soundmixer"); kept and shown, not
    /// played (no mixers yet).
    pub mixer: Option<String>,
    pub loops: Vec<ScapeLoop>,
    pub randoms: Vec<ScapeRandom>,
}

#[derive(Clone, Debug)]
pub struct ScapeLoop {
    /// Index into `clips`.
    pub clip: usize,
    pub volume: Interval,
    /// Percent.
    pub pitch: Interval,
    /// Sound level in dB (0: no falloff).
    pub level: f32,
    /// Index into the applying zone's or emitter's positions; None plays
    /// everywhere, unspatialized.
    pub position: Option<usize>,
    /// Bypasses the room DSP (a `#` wave).
    pub dry: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ScapePosition {
    Ambient,
    Index(usize),
    /// Near the listener in a random direction.
    Random,
}

#[derive(Clone, Debug)]
pub struct ScapeRandom {
    pub clips: Vec<usize>,
    /// Seconds between plays.
    pub time: Interval,
    pub volume: Interval,
    pub pitch: Interval,
    pub level: Interval,
    pub position: ScapePosition,
    /// Bypasses the room DSP (every wave marked `#`).
    pub dry: bool,
}

/// A box that selects a soundscape for a listener inside it (Source
/// trigger_soundscape); the most recently entered one wins.
#[derive(Clone, Debug)]
pub struct SoundscapeZone {
    pub min: Vec3,
    pub max: Vec3,
    pub scape: usize,
    pub positions: Vec<Option<Vec3>>,
    /// The trigger entity (index into `MapData::entities`): with the logic
    /// layer, its touch code says when the listener is inside
    /// (`SoundscapeTouches`), not the box.
    pub entity: Option<usize>,
}

/// A point that selects a soundscape within its radius (env_soundscape).
#[derive(Clone, Debug)]
pub struct SoundscapeEmitter {
    pub at: Vec3,
    /// Meters; None: unlimited.
    pub radius: Option<f32>,
    pub scape: usize,
    pub positions: Vec<Option<Vec3>>,
    /// The entity (index into `MapData::entities`), which the logic
    /// enables and disables (`SoundscapeSwitches`).
    pub entity: Option<usize>,
    /// Disabled at map start (StartDisabled).
    pub start_disabled: bool,
}

impl MapSounds {
    pub fn entry(&self, name: &str) -> Option<&MapSoundEntry> {
        self.entries.get(&name.to_lowercase())
    }
    pub fn surface(&self, name: &str) -> Option<&MapSurface> {
        self.surfaces.get(&name.to_lowercase())
    }
}

/// Play a named sound.
#[derive(Message, Clone, Debug)]
pub struct PlaySound {
    pub entry: String,
    /// Where (engine space); None plays it unspatialized.
    pub at: Option<Vec3>,
    /// Replaces the entry's volume (footsteps compute their own).
    pub volume: Option<f32>,
    /// Replaces the entry's pitch (percent; rules that draw their own).
    pub pitch: Option<f32>,
    /// Who makes it; with `channel`, a new sound replaces the old one.
    pub source: Option<Entity>,
    pub channel: Option<u8>,
}

impl PlaySound {
    pub fn at(entry: impl Into<String>, at: Vec3) -> Self {
        Self {
            entry: entry.into(),
            at: Some(at),
            volume: None,
            pitch: None,
            source: None,
            channel: None,
        }
    }

    /// Unspatialized (announcer, interface).
    pub fn ui(entry: impl Into<String>) -> Self {
        Self {
            at: None,
            ..Self::at(entry, Vec3::ZERO)
        }
    }
}

/// A sound of the game's rules (the bomb, hostages), not of anyone's
/// predicted action: played here (`relay_game_sounds` turns it into a
/// `PlaySound`) and, on a network server, sent to every client (`net`).
#[derive(Message, Clone, Debug)]
pub struct GameSound(pub PlaySound);

/// A sound of a character's own action that its network client predicts
/// (the bomb's key presses while arming, a defuse starting): played here
/// like a `GameSound`; a network server sends it to every client but
/// `by`'s, which played it when it predicted the action (`net`).
#[derive(Message, Clone, Debug)]
pub struct PredictedSound {
    pub by: Entity,
    pub sound: PlaySound,
}

/// Play the rules' sounds here (and characters' predicted ones).
pub fn relay_game_sounds(
    mut sounds: MessageReader<GameSound>,
    mut predicted: MessageReader<PredictedSound>,
    mut play: MessageWriter<PlaySound>,
) {
    for s in predicted.read() {
        play.write(s.sound.clone());
    }
    for s in sounds.read() {
        play.write(s.0.clone());
    }
}

/// The loaded map's sounds.
#[derive(Resource, Clone)]
pub struct SoundBank(pub Arc<MapSounds>);

/// Meters per Source unit.
pub(crate) const METERS_PER_UNIT: f32 = 0.0254;
/// Distances closer than this don't get louder (units).
const MIN_DISTANCE: f32 = 36.0;

/// Gain at `units` from the source (spec open question 1: hypotheses H1
/// for sound levels, H2 for attenuation entries, until measured).
pub fn distance_gain(level: SoundLevel, units: f32) -> f32 {
    match level {
        SoundLevel::Db(l) if l <= 0.0 => 1.0,
        SoundLevel::Db(l) => (10f32.powf((l - 60.0) / 20.0) * MIN_DISTANCE / units.max(MIN_DISTANCE)).min(1.0),
        SoundLevel::Attenuation(a) if a <= 0.0 => 1.0,
        SoundLevel::Attenuation(a) => (1.0 - units * a / 1000.0).clamp(0.0, 1.0),
    }
}

/// World triangles tagged with their surface, on a horizontal grid, to
/// find the surface under a point (footsteps).
#[derive(Resource, Default)]
pub struct SurfaceGrid {
    names: Vec<String>,
    /// Triangle corners and surface index.
    tris: Vec<([Vec3; 3], u16)>,
    grid: HashMap<(i32, i32), Vec<u32>>,
}

impl SurfaceGrid {
    const CELL: f32 = 2.0;

    pub fn new(data: &super::MapData) -> Self {
        let mut me = Self::default();
        let mut index: HashMap<String, u16> = HashMap::new();
        for m in data
            .meshes
            .iter()
            .filter(|m| !m.skybox && m.entity.is_none() && !m.material.starts_with("decal:"))
        {
            let Some(surface) = &m.surface else { continue };
            let id = *index.entry(surface.clone()).or_insert_with(|| {
                me.names.push(surface.clone());
                (me.names.len() - 1) as u16
            });
            for t in m.indices.as_chunks::<3>().0 {
                let tri = t.map(|i| Vec3::from(m.positions[i as usize]));
                let i = me.tris.len() as u32;
                let lo = tri[0].min(tri[1]).min(tri[2]);
                let hi = tri[0].max(tri[1]).max(tri[2]);
                for gx in (lo.x / Self::CELL).floor() as i32..=(hi.x / Self::CELL).floor() as i32 {
                    for gz in (lo.z / Self::CELL).floor() as i32..=(hi.z / Self::CELL).floor() as i32 {
                        me.grid.entry((gx, gz)).or_default().push(i);
                    }
                }
                me.tris.push((tri, id));
            }
        }
        me
    }

    /// The surface of the highest upward-facing triangle under `p` (engine
    /// space), at most `reach` meters below it.
    pub fn below(&self, p: Vec3, reach: f32) -> Option<&str> {
        let key = ((p.x / Self::CELL).floor() as i32, (p.z / Self::CELL).floor() as i32);
        let mut best: Option<(f32, u16)> = None;
        for &i in self.grid.get(&key)? {
            let ([a, b, c], id) = self.tris[i as usize];
            let n = (b - a).cross(c - a);
            if n.y <= 1e-6 {
                continue;
            }
            // Barycentric in the horizontal plane.
            let (a2, b2, c2, q) = (a.xz(), b.xz(), c.xz(), p.xz());
            let area = (b2 - a2).perp_dot(c2 - a2);
            if area.abs() < 1e-9 {
                continue;
            }
            let w1 = (q - a2).perp_dot(c2 - a2) / area;
            let w2 = (b2 - a2).perp_dot(q - a2) / area;
            if w1 < 0.0 || w2 < 0.0 || w1 + w2 > 1.0 {
                continue;
            }
            let y = a.y + (b.y - a.y) * w1 + (c.y - a.y) * w2;
            if y <= p.y + 0.01 && p.y - y <= reach && best.is_none_or(|(by, _)| y > by) {
                best = Some((y, id));
            }
        }
        best.map(|(_, id)| self.names[id as usize].as_str())
    }

    /// The surface of the triangle nearest to `p` within `reach` meters
    /// (bullet impacts on walls, contacts).
    pub fn nearest(&self, p: Vec3, reach: f32) -> Option<&str> {
        let lo = ((p.xz() - reach) / Self::CELL).floor().as_ivec2();
        let hi = ((p.xz() + reach) / Self::CELL).floor().as_ivec2();
        let mut best: Option<(f32, u16)> = None;
        for gx in lo.x..=hi.x {
            for gz in lo.y..=hi.y {
                for &i in self.grid.get(&(gx, gz)).into_iter().flatten() {
                    let (tri, id) = self.tris[i as usize];
                    let d = closest_on_triangle(p, tri).distance_squared(p);
                    if d <= reach * reach && best.is_none_or(|(bd, _)| d < bd) {
                        best = Some((d, id));
                    }
                }
            }
        }
        best.map(|(_, id)| self.names[id as usize].as_str())
    }
}

/// The point of triangle `t` closest to `p`.
fn closest_on_triangle(p: Vec3, [a, b, c]: [Vec3; 3]) -> Vec3 {
    let (ab, ac, ap) = (b - a, c - a, p - a);
    let (d1, d2) = (ab.dot(ap), ac.dot(ap));
    if d1 <= 0.0 && d2 <= 0.0 {
        return a;
    }
    let bp = p - b;
    let (d3, d4) = (ab.dot(bp), ac.dot(bp));
    if d3 >= 0.0 && d4 <= d3 {
        return b;
    }
    let vc = d1 * d4 - d3 * d2;
    if vc <= 0.0 && d1 >= 0.0 && d3 <= 0.0 {
        return a + ab * (d1 / (d1 - d3));
    }
    let cp = p - c;
    let (d5, d6) = (ab.dot(cp), ac.dot(cp));
    if d6 >= 0.0 && d5 <= d6 {
        return c;
    }
    let vb = d5 * d2 - d1 * d6;
    if vb <= 0.0 && d2 >= 0.0 && d6 <= 0.0 {
        return a + ac * (d2 / (d2 - d6));
    }
    let va = d3 * d6 - d5 * d4;
    if va <= 0.0 && d4 - d3 >= 0.0 && d5 - d6 >= 0.0 {
        return b + (c - b) * ((d4 - d3) / ((d4 - d3) + (d5 - d6)));
    }
    let denom = 1.0 / (va + vb + vc);
    a + ab * (vb * denom) + ac * (vc * denom)
}

/// Who a long-lived sound belongs to (chosen by its owner).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct SoundKey(pub u64);

/// Keys with this bit (and not bit 63, the logic's) belong to the
/// soundscape's loops (`audio::soundscape`), which `StopAll` leaves alone.
pub const SOUNDSCAPE_KEYS: u64 = 1 << 62;

impl SoundKey {
    pub(crate) fn soundscape(self) -> bool {
        self.0 >> 62 == 1
    }
}

/// Start, change or stop a long-lived sound.
#[derive(Message, Clone, Debug)]
pub enum SoundControl {
    /// Replaces whatever plays under the same key.
    Start(StartSound),
    /// New volume and/or pitch (percent) for a playing sound.
    Change {
        key: SoundKey,
        volume: Option<f32>,
        pitch: Option<f32>,
    },
    Stop(SoundKey),
    /// Every long-lived sound but the soundscape's loops (map change,
    /// round restart).
    StopAll,
}

/// A long-lived sound to start.
#[derive(Clone, Debug, Default)]
pub struct StartSound {
    pub key: SoundKey,
    /// A sound entry of the map's `SoundBank` (games add raw files as
    /// entries of their own).
    pub entry: String,
    /// Where (engine space); None plays it unspatialized.
    pub at: Option<Vec3>,
    /// Plays from this entity's position while it exists (else `at`).
    pub follow: Option<Entity>,
    /// Override the entry's volume, pitch (percent) and level.
    pub volume: Option<f32>,
    pub pitch: Option<f32>,
    pub level: Option<SoundLevel>,
    /// Play this clip of the bank (`entry` then only names it).
    pub clip: Option<usize>,
    /// Loop even without a loop point (from the start).
    pub looping: bool,
    /// Bypass the room (Source's `#` waves).
    pub dry: bool,
}
