//! Voice chat on the client (docs/plans/active/voice-chat.md): the
//! microphone, the Opus codec and the talkers' playback. The network's
//! side (what goes on the wire, the server's relay and who hears whom) is
//! `net::voice`.
//!
//! - **Talking**: `+voicerecord` (CS:S binds it to K) records while held;
//!   with `voice_vox 1` (open microphone) the microphone stays open and a
//!   20 ms frame whose loudest sample reaches `voice_threshold` opens the
//!   gate for `VOX_HANG` seconds of audio (frames under it while it is
//!   closed are dropped, the key still talks).
//!   The microphone's samples (48 kHz mono, `VoiceInput`), times `BOOST`
//!   with `voice_boost 1` (the Voice tab's "Boost microphone gain"), are
//!   cut into
//!   20 ms frames, each encoded with Opus (VoIP, `BITRATE`) and sent as
//!   `net::voice::VoiceData`, at most `net::voice::MAX_FRAMES` a frame;
//!   the rest of a key press's audio follows in the next frames, and what
//!   is left when the key comes up is padded to a whole frame. Nothing is
//!   sent outside a network game (no one to hear), nor with `voice_enable`
//!   or `voice_modenable` 0. `voice_loopback 1` plays one's own voice
//!   back (decoded here: the server never sends a talker its own).
//! - **Microphone test** (`voice_test`, the Voice tab's Test Microphone):
//!   the microphone open, each frame encoded and decoded here and played
//!   back to the speakers (as `voice_loopback` does), nothing sent.
//!   `VoiceLevel` is the last frame's loudest sample (0 to 1, boost
//!   included) while the microphone is open, else 0: the tab's meter.
//! - **Hearing**: each talker's `net::voice::VoiceRelay` is decoded with
//!   its own decoder, unless its player is muted (`net::voice::VoiceMutes`,
//!   the player list's mute: dropped undecoded); a gap of up to `MAX_CONCEALED` frames (lost
//!   packets) is filled by Opus's concealment, older or repeated frames
//!   are dropped. The samples go to `VoiceOutput` at `voice_scale` times
//!   the master volume (Bevy's `GlobalVolume`), flat (CS:S's voice isn't
//!   positional).
//! - **Devices**: `VoiceInput` and `VoiceOutput` are traits, so tests use
//!   the fakes (`FakeInput`, `FakeOutput`) headless. The system's default
//!   microphone and speakers (cpal) open when first needed, unless
//!   `VoiceDevices::open_system` is off; each runs on its own thread,
//!   which owns the stream, resampled linearly to and from 48 kHz. The
//!   speakers' mixer holds each talker's samples and starts a talker after
//!   `PREBUFFER` seconds of them (a jitter buffer).
//!
//! Voice chat on the client: `+voicerecord`, `voice_vox` (open microphone,
//! `voice_threshold`), the microphone and speakers
//! (`VoiceInput`, `VoiceOutput`: cpal's default devices, fakes for tests),
//! Opus at 48 kHz mono in 20 ms frames (`mousiki`), per-talker decoders with
//! loss concealment, muted players dropped (`VoiceMutes`), a jitter-buffered
//! mixer, `voice_enable`, `voice_modenable`, `voice_scale`, `voice_loopback`,
//! `voice_boost`, `voice_test` and `VoiceLevel` (the Voice tab's microphone
//! test and meter).
//!
//! Known shortcuts (docs/tech-debt.md): Voice chat (`client::voice`): the
//! client cvars' defaults (`voice_enable` 1, `voice_modenable` 1,
//! `voice_scale` 1, `voice_loopback` 0, `voice_vox` 0) are guesses (the
//! dedicated server doesn't list client cvars: a Windows capture);
//! `voice_threshold` (4000, a frame's loudest sample) and the gate's
//! `VOX_HANG` (0.5 s) are ours (Source's open microphone's measure and
//! hold aren't known); Opus at 24 kbit/s in wideband (SILK only: `mousiki`'s CELT layer
//! trips its own debug assertion) is ours
//! (CS:S's `sv_voicecodec` is Steam's voice, also Opus-based); linear
//! resampling; a 60 ms jitter buffer and at most 0.5 s of microphone backlog
//! (ours); the system's default devices only (no device choice);
//! `voice_boost` (CS:S's box sets the sound card's microphone boost; here
//! x4, +12 dB) and `voice_test` (CS:S's button runs its voice tweak mode)
//! are ours; `voice_loopback` decodes locally (Source's server echoes it).

use std::{
    collections::{HashMap, VecDeque},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
        mpsc,
    },
    time::Duration,
};

use bevy::{audio::GlobalVolume, prelude::*};
use mousiki::{Application, Bandwidth, Bitrate, Channels, Decoder, Encoder, Signal};

use crate::{
    console::{ConsoleAppExt, resource_cvar},
    core::{LocalPlayer, NetRole},
    net::{
        NetCharacter,
        voice::{MAX_FRAMES, VoiceActivity, VoiceData, VoiceMutes, VoiceRelay},
    },
};

/// The codec's sample rate (Hz) and frame (samples: 20 ms).
pub const RATE: u32 = 48_000;
pub const FRAME: usize = 960;
/// Opus's bit rate (bit/s): clear speech in ~3.7 kB/s (ours).
pub const BITRATE: i32 = 24_000;
/// Most lost frames in a row filled by concealment; a longer gap starts
/// afresh.
pub const MAX_CONCEALED: u16 = 5;
/// Seconds of a talker's voice held before it starts playing.
pub const PREBUFFER: f32 = 0.06;
/// Most microphone audio waiting to be sent (s): older is dropped.
const MAX_BACKLOG: usize = RATE as usize / 2;
/// How long the open microphone's gate stays open after a frame at
/// `voice_threshold` (s of audio; ours).
pub const VOX_HANG: f32 = 0.5;
/// The microphone's gain with `voice_boost 1` (+12 dB; ours).
pub const BOOST: f32 = 4.0;

/// A microphone: 48 kHz mono samples.
pub trait VoiceInput: Send + Sync + 'static {
    /// Append what was captured since the last call.
    fn read(&mut self, out: &mut Vec<i16>);
    /// Recording starts (true) or stops: a device may drop what it
    /// captured while not recording.
    fn set_active(&mut self, _on: bool) {}
}

/// Speakers: each talker's decoded voice, 48 kHz mono, at `gain`.
pub trait VoiceOutput: Send + Sync + 'static {
    fn play(&mut self, talker: Entity, pcm: &[i16], gain: f32);
}

/// A microphone for tests: what they push is what it hears.
#[derive(Clone, Default)]
pub struct FakeInput(pub Arc<Mutex<VecDeque<i16>>>);

impl FakeInput {
    /// `seconds` of a sine tone at `hz` (amplitude 8000).
    pub fn push_tone(&self, hz: f32, seconds: f32) {
        let n = (seconds * RATE as f32) as usize;
        let mut q = self.0.lock().unwrap();
        q.extend((0..n).map(|i| ((i as f32 / RATE as f32 * hz * std::f32::consts::TAU).sin() * 8000.0) as i16));
    }
}

impl VoiceInput for FakeInput {
    fn read(&mut self, out: &mut Vec<i16>) {
        out.extend(self.0.lock().unwrap().drain(..));
    }
}

/// Speakers for tests: every talker's samples as played (gain applied).
#[derive(Clone, Default)]
pub struct FakeOutput(pub Arc<Mutex<HashMap<Entity, Vec<i16>>>>);

impl FakeOutput {
    /// Samples played for `talker` so far.
    pub fn samples(&self, talker: Entity) -> usize {
        self.0.lock().unwrap().get(&talker).map_or(0, Vec::len)
    }

    /// Samples played for anyone so far.
    pub fn total(&self) -> usize {
        self.0.lock().unwrap().values().map(Vec::len).sum()
    }
}

impl VoiceOutput for FakeOutput {
    fn play(&mut self, talker: Entity, pcm: &[i16], gain: f32) {
        let mut m = self.0.lock().unwrap();
        m.entry(talker)
            .or_default()
            .extend(pcm.iter().map(|s| (*s as f32 * gain).clamp(-32768.0, 32767.0) as i16));
    }
}

/// The microphone and speakers in use.
#[derive(Resource)]
pub struct VoiceDevices {
    pub input: Option<Box<dyn VoiceInput>>,
    pub output: Option<Box<dyn VoiceOutput>>,
    /// Open the system's default devices when first needed (off in tests,
    /// which put fakes in).
    pub open_system: bool,
    input_failed: bool,
    output_failed: bool,
}

impl Default for VoiceDevices {
    fn default() -> Self {
        Self {
            input: None,
            output: None,
            open_system: true,
            input_failed: false,
            output_failed: false,
        }
    }
}

impl VoiceDevices {
    /// Fakes in place of the system's devices.
    pub fn fake(input: FakeInput, output: FakeOutput) -> Self {
        Self {
            input: Some(Box::new(input)),
            output: Some(Box::new(output)),
            open_system: false,
            ..default()
        }
    }
}

/// `+voicerecord` held.
#[derive(Resource, Default, Debug, Clone, Copy, PartialEq, Eq)]
pub struct VoiceRecord(pub bool);

/// The microphone test runs (`voice_test`, the Voice tab's Test
/// Microphone): one's voice played back, nothing sent.
#[derive(Resource, Default, Debug, Clone, Copy, PartialEq, Eq)]
pub struct VoiceTest(pub bool);

/// The microphone's level: the last 20 ms frame's loudest sample (0 to 1,
/// boost included) while it is open, else 0.
#[derive(Resource, Default, Debug, Clone, Copy, PartialEq)]
pub struct VoiceLevel(pub f32);

/// The player's voice settings.
#[derive(Resource, Clone, Copy, Debug, PartialEq)]
pub struct VoiceSettings {
    /// `voice_enable`: 0 neither sends nor plays voice.
    pub enable: u8,
    /// `voice_modenable`: the Voice tab's "Enable voice in this game".
    pub modenable: u8,
    /// `voice_scale`: the talkers' volume (times the master volume).
    pub scale: f32,
    /// `voice_loopback`: 1 plays one's own voice back.
    pub loopback: u8,
    /// `voice_vox`: 1 is an open microphone (no key; `threshold` gates it).
    pub vox: u8,
    /// `voice_threshold`: the loudest sample of a 20 ms frame (of 32767)
    /// that opens the open microphone's gate.
    pub threshold: f32,
    /// `voice_boost` (ours; the Voice tab's "Boost microphone gain"): 1
    /// multiplies the microphone by `BOOST`.
    pub boost: u8,
}

impl Default for VoiceSettings {
    fn default() -> Self {
        Self {
            enable: 1,
            modenable: 1,
            scale: 1.0,
            loopback: 0,
            vox: 0,
            threshold: 4000.0,
            boost: 0,
        }
    }
}

impl VoiceSettings {
    pub fn on(&self) -> bool {
        self.enable != 0 && self.modenable != 0
    }
}

/// The microphone's side: the samples not yet sent, the encoder, the next
/// frame's number.
#[derive(Resource)]
struct Capture {
    pending: Vec<i16>,
    encoder: Mutex<Encoder>,
    /// Our own voice decoded for `voice_loopback`.
    loopback: Mutex<Decoder>,
    seq: u16,
    /// The microphone was open last frame.
    was_open: bool,
    /// The talk key was held last frame.
    was_key: bool,
    /// The key came up: the rest of its audio is still going out.
    flushing: bool,
    /// Frames the open microphone's gate stays open for.
    vox_left: u32,
}

/// Each talker's decoder and the frame number it expects next.
struct Talker {
    decoder: Decoder,
    next: Option<u16>,
    last: f64,
}

#[derive(Resource, Default)]
struct Talkers(HashMap<Entity, Mutex<Talker>>);

/// A new Opus encoder as voice uses it.
/// Speech in wideband (16 kHz audio bandwidth): Opus's SILK layer alone.
/// Wider bands bring in its CELT layer (hybrid mode), where `mousiki`
/// 0.2.1 trips a debug assertion of its own (overlapping band windows).
pub fn encoder() -> Encoder {
    let mut e = Encoder::builder(RATE, Channels::Mono, Application::Voip)
        .bitrate(Bitrate::Bits(BITRATE))
        .build()
        .expect("Opus encoder");
    e.set_max_bandwidth(Bandwidth::Wideband).expect("Opus bandwidth");
    e.set_signal(Signal::Voice).expect("Opus signal");
    e
}

/// A new Opus decoder as voice uses it.
pub fn decoder() -> Decoder {
    Decoder::new(RATE, Channels::Mono).expect("Opus decoder")
}

/// Voice chat on the client: `+voicerecord`, the cvars, capture and
/// playback.
pub struct VoicePlugin;

impl Plugin for VoicePlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<VoiceDevices>()
            .init_resource::<VoiceRecord>()
            .init_resource::<VoiceTest>()
            .init_resource::<VoiceLevel>()
            .init_resource::<VoiceSettings>()
            .init_resource::<VoiceActivity>()
            .init_resource::<VoiceMutes>()
            .init_resource::<Talkers>()
            .insert_resource(Capture {
                pending: Vec::new(),
                encoder: Mutex::new(encoder()),
                loopback: Mutex::new(decoder()),
                seq: 0,
                was_open: false,
                was_key: false,
                flushing: false,
                vox_left: 0,
            })
            .add_systems(Update, (capture, play));
        for (sign, on) in [("+", true), ("-", false)] {
            app.console_command(
                &format!("{sign}voicerecord"),
                if on { "Start talking (voice chat; K in CS:S)." } else { "Stop talking." },
                move |w, _| {
                    w.resource_mut::<VoiceRecord>().0 = on;
                    Ok(None)
                },
            );
        }
        app.console_command(
            "voice_test",
            "voice_test [0|1]: test the microphone: hear yourself, nothing sent (the Voice tab's Test Microphone; \
             toggles without a value).",
            |w, a| {
                let on = match a.first() {
                    Some(v) => v.parse::<f32>().map_err(|_| "voice_test [0|1]")? != 0.0,
                    None => !w.resource::<VoiceTest>().0,
                };
                w.resource_mut::<VoiceTest>().0 = on;
                Ok(None)
            },
        );
        resource_cvar::<VoiceSettings, u8>(app, "voice_enable", "1: voice chat on (sent and heard); 0: off.", |s| {
            &mut s.enable
        });
        resource_cvar::<VoiceSettings, u8>(
            app,
            "voice_modenable",
            "1: voice in this game (the Voice tab's \"Enable voice in this game\").",
            |s| &mut s.modenable,
        );
        resource_cvar::<VoiceSettings, f32>(app, "voice_scale", "Other players' voice volume (times the master volume).", |s| {
            &mut s.scale
        });
        resource_cvar::<VoiceSettings, u8>(app, "voice_loopback", "1: hear your own voice while you talk.", |s| {
            &mut s.loopback
        });
        resource_cvar::<VoiceSettings, u8>(
            app,
            "voice_vox",
            "1: open microphone: your voice goes out whenever it is louder than voice_threshold (no key).",
            |s| &mut s.vox,
        );
        resource_cvar::<VoiceSettings, f32>(
            app,
            "voice_threshold",
            "The open microphone's (voice_vox) level: a 20 ms frame's loudest sample, of 32767.",
            |s| &mut s.threshold,
        );
        resource_cvar::<VoiceSettings, u8>(
            app,
            "voice_boost",
            "1: boost the microphone's gain (+12 dB; the Voice tab's \"Boost microphone gain\").",
            |s| &mut s.boost,
        );
        let mut console = app.world_mut().resource_mut::<crate::console::Console>();
        for c in ["voice_enable", "voice_modenable", "voice_scale", "voice_vox", "voice_threshold", "voice_boost"] {
            console.archive(c);
        }
        console.set_range("voice_scale", 0.0, 1.0);
        console.set_range("voice_threshold", 0.0, 32767.0);
    }
}

/// The master volume heard (Bevy's `GlobalVolume`: `volume`, silent when
/// muted for focus).
fn master(global: Option<&GlobalVolume>) -> f32 {
    global.map_or(1.0, |g| g.volume.to_linear())
}

/// Open the system's speakers when first needed.
fn ensure_output(devices: &mut VoiceDevices) {
    if devices.output.is_none() && devices.open_system && !devices.output_failed {
        match cpal_devices::open_output() {
            Ok(output) => devices.output = Some(Box::new(output)),
            Err(e) => {
                warn!("voice: no speakers: {e}");
                devices.output_failed = true;
            }
        }
    }
}

/// The loudest sample of a frame, 0 to 1.
pub fn peak(pcm: &[i16]) -> f32 {
    pcm.iter().map(|s| s.unsigned_abs()).max().unwrap_or(0) as f32 / 32768.0
}

/// Record, encode and send while `+voicerecord` is held or the open
/// microphone's gate is open; play back for the microphone test.
#[allow(clippy::too_many_arguments)]
fn capture(
    record: Res<VoiceRecord>,
    test: Res<VoiceTest>,
    settings: Res<VoiceSettings>,
    role: Res<NetRole>,
    global: Option<Res<GlobalVolume>>,
    me: Query<Entity, With<LocalPlayer>>,
    mut devices: ResMut<VoiceDevices>,
    mut state: ResMut<Capture>,
    mut activity: ResMut<VoiceActivity>,
    mut level: ResMut<VoiceLevel>,
    mut out: MessageWriter<VoiceData>,
) {
    let key = record.0 && settings.on();
    let vox = settings.vox != 0 && settings.on();
    let open = key || vox || test.0;
    let devices = &mut *devices;
    if open && devices.input.is_none() && devices.open_system && !devices.input_failed {
        match cpal_devices::open_input() {
            Ok(input) => devices.input = Some(Box::new(input)),
            Err(e) => {
                warn!("voice: no microphone: {e}");
                devices.input_failed = true;
            }
        }
    }
    if test.0 {
        ensure_output(devices);
    }
    let state = &mut *state;
    if open != state.was_open {
        state.was_open = open;
        if let Some(input) = devices.input.as_mut() {
            input.set_active(open);
        }
        if open && !state.flushing {
            state.pending.clear();
        }
        state.vox_left = 0;
    }
    if state.was_key && !key && !vox && !state.pending.is_empty() {
        // The key came up: what is left goes out, padded to a whole frame.
        state.flushing = true;
        let n = state.pending.len().div_ceil(FRAME) * FRAME;
        state.pending.resize(n, 0);
    }
    state.was_key = key;
    let Some(input) = devices.input.as_mut() else {
        activity.local = false;
        level.set_if_neq(VoiceLevel(0.0));
        return;
    };
    let mut fresh = Vec::new();
    input.read(&mut fresh);
    if open {
        state.pending.extend_from_slice(&fresh);
        if state.pending.len() > MAX_BACKLOG {
            let drop = state.pending.len() - MAX_BACKLOG;
            state.pending.drain(..drop.div_ceil(FRAME) * FRAME);
        }
    } else if !state.flushing {
        state.pending.clear();
    }
    let hang = (VOX_HANG * RATE as f32 / FRAME as f32).round() as u32;
    let gain = if settings.boost != 0 { BOOST } else { 1.0 };
    let mut frames = Vec::new();
    let mut back = Vec::new();
    let mut loudest = None;
    while state.pending.len() >= FRAME && frames.len() < MAX_FRAMES {
        let mut pcm: Vec<i16> = state.pending.drain(..FRAME).collect();
        if gain != 1.0 {
            for x in &mut pcm {
                *x = (*x as f32 * gain).clamp(-32768.0, 32767.0) as i16;
            }
        }
        let p = peak(&pcm);
        loudest = Some(loudest.map_or(p, |l: f32| l.max(p)));
        let gate = vox && {
            if p * 32768.0 >= settings.threshold {
                state.vox_left = hang;
            }
            let on = state.vox_left > 0;
            state.vox_left = state.vox_left.saturating_sub(1);
            on
        };
        let send = key || state.flushing || gate;
        // Played back: the microphone test, or `voice_loopback` while
        // talking.
        let echo = test.0 || (send && settings.loopback != 0);
        if !send && !echo {
            continue;
        }
        let mut packet = [0u8; 1500];
        match state.encoder.get_mut().unwrap().encode(&pcm, &mut packet) {
            Ok(n) => {
                if echo {
                    back.push(packet[..n].to_vec());
                }
                if send {
                    frames.push(packet[..n].to_vec());
                }
            }
            Err(e) => warn!("voice: encoding failed: {e:?}"),
        }
    }
    if state.pending.is_empty() {
        state.flushing = false;
    }
    activity.local = key || state.flushing || (vox && state.vox_left > 0);
    let shown = if open { loudest.unwrap_or(level.0) } else { 0.0 };
    level.set_if_neq(VoiceLevel(shown));
    if !back.is_empty()
        && let Some(output) = devices.output.as_mut()
    {
        let decoder = state.loopback.get_mut().unwrap();
        let gain = settings.scale * master(global.as_deref());
        let me = me.iter().next().unwrap_or(Entity::PLACEHOLDER);
        for f in &back {
            let mut pcm = [0i16; FRAME];
            if let Ok(n) = decoder.decode(f, &mut pcm, false) {
                output.play(me, &pcm[..n], gain);
            }
        }
    }
    if frames.is_empty() {
        return;
    }
    let seq = state.seq;
    state.seq = state.seq.wrapping_add(frames.len() as u16);
    // Outside a network game nobody hears it.
    if *role != NetRole::Standalone {
        out.write(VoiceData { seq, frames });
    }
}

/// Decode the voice that reached us and play it (not muted players').
#[allow(clippy::too_many_arguments)]
fn play(
    mut relays: MessageReader<VoiceRelay>,
    settings: Res<VoiceSettings>,
    mutes: Res<VoiceMutes>,
    characters: Query<&NetCharacter>,
    global: Option<Res<GlobalVolume>>,
    time: Res<Time<Real>>,
    mut devices: ResMut<VoiceDevices>,
    mut talkers: ResMut<Talkers>,
) {
    let now = time.elapsed_secs_f64();
    talkers.0.retain(|_, t| now - t.get_mut().unwrap().last < 5.0);
    if !settings.on() {
        relays.clear();
        return;
    }
    let devices = &mut *devices;
    let gain = settings.scale.max(0.0) * master(global.as_deref());
    for r in relays.read() {
        if r.speaker == Entity::PLACEHOLDER || mutes.muted(characters.get(r.speaker).ok()) {
            continue;
        }
        ensure_output(devices);
        let Some(output) = devices.output.as_mut() else { continue };
        let talker = talkers.0.entry(r.speaker).or_insert_with(|| {
            Mutex::new(Talker {
                decoder: decoder(),
                next: None,
                last: now,
            })
        });
        let talker = talker.get_mut().unwrap();
        talker.last = now;
        let mut pcm = Vec::with_capacity(FRAME * r.frames.len());
        for (i, f) in r.frames.iter().enumerate() {
            let seq = r.seq.wrapping_add(i as u16);
            let ahead = talker.next.map_or(0, |n| seq.wrapping_sub(n) as i16);
            if ahead < 0 {
                // Late or repeated.
                continue;
            }
            if ahead > 0 && ahead as u16 <= MAX_CONCEALED {
                for _ in 0..ahead {
                    let mut lost = [0i16; FRAME];
                    if let Ok(n) = talker.decoder.conceal(&mut lost) {
                        pcm.extend_from_slice(&lost[..n]);
                    }
                }
            }
            let mut frame = [0i16; FRAME];
            match talker.decoder.decode(f, &mut frame, false) {
                Ok(n) => pcm.extend_from_slice(&frame[..n]),
                Err(e) => debug!("voice: a bad frame: {e:?}"),
            }
            talker.next = Some(seq.wrapping_add(1));
        }
        if !pcm.is_empty() {
            output.play(r.speaker, &pcm, gain);
        }
    }
}

/// Linear resampling of a stream, one sample at a time.
#[derive(Clone, Debug)]
pub struct Resampler {
    /// Input samples per output sample.
    step: f64,
    /// Where the next output falls after `prev` (0..1 to the next input).
    pos: f64,
    prev: f32,
}

impl Resampler {
    pub fn new(from: u32, to: u32) -> Self {
        Self {
            step: from as f64 / to.max(1) as f64,
            pos: 0.0,
            prev: 0.0,
        }
    }

    /// Feed one input sample; the output samples it completes go to `out`.
    pub fn push(&mut self, x: f32, mut out: impl FnMut(f32)) {
        while self.pos < 1.0 {
            out(self.prev + (x - self.prev) * self.pos as f32);
            self.pos += self.step;
        }
        self.pos -= 1.0;
        self.prev = x;
    }
}

/// The system's default microphone and speakers through cpal, each on a
/// thread that owns its stream (cpal's streams aren't `Send` everywhere).
mod cpal_devices {
    use cpal::{
        FromSample, SampleFormat, SizedSample,
        traits::{DeviceTrait, HostTrait, StreamTrait},
    };

    use super::*;

    /// Start a thread that builds a stream with `build` and keeps it until
    /// the returned sender is dropped.
    fn run<F>(name: &str, build: F) -> Result<mpsc::Sender<()>, String>
    where
        F: FnOnce() -> Result<cpal::Stream, String> + Send + 'static,
    {
        let (ready_tx, ready_rx) = mpsc::channel();
        let (stop_tx, stop_rx) = mpsc::channel::<()>();
        std::thread::Builder::new()
            .name(name.into())
            .spawn(move || match build() {
                Ok(stream) => {
                    let _ = ready_tx.send(Ok(()));
                    // Until the device's owner is dropped.
                    let _ = stop_rx.recv();
                    drop(stream);
                }
                Err(e) => {
                    let _ = ready_tx.send(Err(e));
                }
            })
            .map_err(|e| e.to_string())?;
        ready_rx
            .recv_timeout(Duration::from_secs(5))
            .map_err(|_| "the device didn't open".to_string())??;
        Ok(stop_tx)
    }

    /// The default microphone, down-mixed and resampled to 48 kHz mono.
    pub struct Mic {
        queue: Arc<Mutex<VecDeque<i16>>>,
        active: Arc<AtomicBool>,
        _stop: mpsc::Sender<()>,
    }

    impl VoiceInput for Mic {
        fn read(&mut self, out: &mut Vec<i16>) {
            out.extend(self.queue.lock().unwrap().drain(..));
        }

        fn set_active(&mut self, on: bool) {
            self.active.store(on, Ordering::Relaxed);
            if !on {
                self.queue.lock().unwrap().clear();
            }
        }
    }

    fn input_stream<T>(
        device: &cpal::Device,
        config: &cpal::StreamConfig,
        queue: Arc<Mutex<VecDeque<i16>>>,
        active: Arc<AtomicBool>,
    ) -> Result<cpal::Stream, cpal::BuildStreamError>
    where
        T: SizedSample,
        f32: FromSample<T>,
    {
        let channels = config.channels.max(1) as usize;
        let mut resampler = Resampler::new(config.sample_rate, RATE);
        device.build_input_stream::<T, _, _>(
            config,
            move |data: &[T], _| {
                if !active.load(Ordering::Relaxed) {
                    return;
                }
                let mut q = queue.lock().unwrap();
                for frame in data.chunks(channels) {
                    let mono = frame.iter().map(|s| <f32 as FromSample<T>>::from_sample_(*s)).sum::<f32>() / channels as f32;
                    resampler.push(mono, |x| q.push_back((x.clamp(-1.0, 1.0) * 32767.0) as i16));
                }
                // A second at most if nobody reads.
                let over = q.len().saturating_sub(RATE as usize);
                q.drain(..over);
            },
            |e| warn!("voice: microphone: {e}"),
            None,
        )
    }

    pub fn open_input() -> Result<Mic, String> {
        let queue = Arc::new(Mutex::new(VecDeque::new()));
        let active = Arc::new(AtomicBool::new(true));
        let (q, a) = (queue.clone(), active.clone());
        let stop = run("voice-input", move || {
            let device = cpal::default_host().default_input_device().ok_or("no input device")?;
            let supported = device.default_input_config().map_err(|e| e.to_string())?;
            let config = supported.config();
            let stream = match supported.sample_format() {
                SampleFormat::F32 => input_stream::<f32>(&device, &config, q, a),
                SampleFormat::I16 => input_stream::<i16>(&device, &config, q, a),
                SampleFormat::U16 => input_stream::<u16>(&device, &config, q, a),
                SampleFormat::I32 => input_stream::<i32>(&device, &config, q, a),
                f => return Err(format!("unsupported sample format {f}")),
            }
            .map_err(|e| e.to_string())?;
            stream.play().map_err(|e| e.to_string())?;
            info!("voice: microphone at {} Hz, {} channels", config.sample_rate, config.channels);
            Ok(stream)
        })?;
        Ok(Mic {
            queue,
            active,
            _stop: stop,
        })
    }

    /// One talker's samples at the device's rate, and whether it plays
    /// (after `PREBUFFER` of them).
    struct Stream {
        queue: VecDeque<f32>,
        resampler: Resampler,
        started: bool,
    }

    /// The talkers' streams summed for the device.
    struct Mixer {
        rate: u32,
        streams: HashMap<Entity, Stream>,
    }

    /// The default speakers.
    pub struct Speakers {
        mixer: Arc<Mutex<Mixer>>,
        _stop: mpsc::Sender<()>,
    }

    impl VoiceOutput for Speakers {
        fn play(&mut self, talker: Entity, pcm: &[i16], gain: f32) {
            let mut m = self.mixer.lock().unwrap();
            let rate = m.rate;
            let s = m.streams.entry(talker).or_insert_with(|| Stream {
                queue: VecDeque::new(),
                resampler: Resampler::new(RATE, rate),
                started: false,
            });
            for x in pcm {
                let q = &mut s.queue;
                s.resampler.push(*x as f32 / 32768.0 * gain, |y| q.push_back(y));
            }
            // A second at most behind.
            let over = s.queue.len().saturating_sub(rate as usize);
            s.queue.drain(..over);
        }
    }

    fn output_stream<T>(
        device: &cpal::Device,
        config: &cpal::StreamConfig,
        mixer: Arc<Mutex<Mixer>>,
    ) -> Result<cpal::Stream, cpal::BuildStreamError>
    where
        T: SizedSample + FromSample<f32>,
    {
        let channels = config.channels.max(1) as usize;
        let prebuffer = (config.sample_rate as f32 * PREBUFFER) as usize;
        device.build_output_stream::<T, _, _>(
            config,
            move |data: &mut [T], _| {
                let mut m = mixer.lock().unwrap();
                for s in m.streams.values_mut() {
                    if !s.started && s.queue.len() >= prebuffer {
                        s.started = true;
                    }
                }
                for frame in data.chunks_mut(channels) {
                    let mut sum = 0.0;
                    for s in m.streams.values_mut().filter(|s| s.started) {
                        match s.queue.pop_front() {
                            Some(x) => sum += x,
                            // Drained: wait for a new buffer's worth.
                            None => s.started = false,
                        }
                    }
                    let v = T::from_sample_(sum.clamp(-1.0, 1.0));
                    for c in frame {
                        *c = v;
                    }
                }
                m.streams.retain(|_, s| s.started || !s.queue.is_empty());
            },
            |e| warn!("voice: speakers: {e}"),
            None,
        )
    }

    pub fn open_output() -> Result<Speakers, String> {
        let device = cpal::default_host().default_output_device().ok_or("no output device")?;
        let supported = device.default_output_config().map_err(|e| e.to_string())?;
        let mixer = Arc::new(Mutex::new(Mixer {
            rate: supported.sample_rate(),
            streams: HashMap::new(),
        }));
        let m = mixer.clone();
        let stop = run("voice-output", move || {
            let config = supported.config();
            let stream = match supported.sample_format() {
                SampleFormat::F32 => output_stream::<f32>(&device, &config, m),
                SampleFormat::I16 => output_stream::<i16>(&device, &config, m),
                SampleFormat::U16 => output_stream::<u16>(&device, &config, m),
                SampleFormat::I32 => output_stream::<i32>(&device, &config, m),
                f => return Err(format!("unsupported sample format {f}")),
            }
            .map_err(|e| e.to_string())?;
            stream.play().map_err(|e| e.to_string())?;
            info!("voice: speakers at {} Hz, {} channels", config.sample_rate, config.channels);
            Ok(stream)
        })?;
        Ok(Speakers { mixer, _stop: stop })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A tone through the codec comes back as a tone of about its
    /// loudness, in a few kilobytes a second.
    #[test]
    fn opus_round_trip() {
        let mut enc = encoder();
        let mut dec = decoder();
        let input = FakeInput::default();
        input.push_tone(440.0, 1.0);
        let pcm: Vec<i16> = input.0.lock().unwrap().drain(..).collect();
        let (mut bytes, mut energy, mut n) = (0, 0.0f64, 0);
        for chunk in pcm.chunks(FRAME) {
            let mut packet = [0u8; 1500];
            let len = enc.encode(chunk, &mut packet).unwrap();
            bytes += len;
            let mut out = [0i16; FRAME];
            let got = dec.decode(&packet[..len], &mut out, false).unwrap();
            assert_eq!(got, FRAME);
            energy += out.iter().map(|s| (*s as f64).powi(2)).sum::<f64>();
            n += got;
        }
        let rms = (energy / n as f64).sqrt();
        // The tone's RMS is 8000 / sqrt(2) = 5657; the codec's start-up
        // and smoothing take a little.
        assert!(rms > 4000.0 && rms < 6500.0, "{rms}");
        assert!(bytes < 5000, "{bytes} bytes for a second");
        // Speech-like sound (a gliding tone with noise) and silence encode
        // too, a frame at a time.
        let mut seed = 1u32;
        for f in 0..100 {
            let pcm: Vec<i16> = (0..FRAME)
                .map(|i| {
                    seed = seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
                    let t = (f * FRAME + i) as f32 / RATE as f32;
                    let glide = (t * (200.0 + 3000.0 * t) * std::f32::consts::TAU).sin() * 6000.0;
                    let noise = (seed >> 16) as f32 / 65536.0 * 4000.0 - 2000.0;
                    if f % 10 == 9 { 0 } else { (glide + noise) as i16 }
                })
                .collect();
            let mut packet = [0u8; 1500];
            let len = enc.encode(&pcm, &mut packet).unwrap();
            let mut out = [0i16; FRAME];
            assert_eq!(dec.decode(&packet[..len], &mut out, false).unwrap(), FRAME);
        }
        // Concealing a lost frame gives a frame.
        let mut lost = [0i16; FRAME];
        assert_eq!(dec.conceal(&mut lost).unwrap(), FRAME);
    }

    /// An app with voice chat on fake devices, as a network client.
    fn app(input: FakeInput, output: FakeOutput) -> App {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, crate::console::ConsolePlugin))
            .add_message::<VoiceData>()
            .add_message::<VoiceRelay>()
            .insert_resource(NetRole::Client)
            .add_plugins(VoicePlugin)
            .insert_resource(VoiceDevices::fake(input, output))
            .init_resource::<Sent>()
            .add_systems(PostUpdate, |mut data: MessageReader<VoiceData>, mut sent: ResMut<Sent>| {
                for d in data.read() {
                    sent.0.push(d.frames.len());
                }
            });
        app
    }

    /// The frames of each `VoiceData` sent.
    #[derive(Resource, Default)]
    struct Sent(Vec<usize>);

    impl Sent {
        fn frames(app: &App) -> usize {
            app.world().resource::<Sent>().0.iter().sum()
        }
    }

    /// `n` frames of a tone (amplitude 8000) encoded as a talker sends them.
    fn packets(n: usize) -> Vec<Vec<u8>> {
        let mic = FakeInput::default();
        mic.push_tone(300.0, n as f32 * FRAME as f32 / RATE as f32);
        let pcm: Vec<i16> = mic.0.lock().unwrap().drain(..).collect();
        let mut enc = encoder();
        pcm.chunks(FRAME)
            .map(|c| {
                let mut p = [0u8; 1500];
                let len = enc.encode(c, &mut p).unwrap();
                p[..len].to_vec()
            })
            .collect()
    }

    /// A muted player's voice is never played; others' is; unmuted, it is
    /// again. Bots (no user id) can't be muted.
    #[test]
    fn a_muted_talker_is_not_played() {
        let out = FakeOutput::default();
        let mut app = app(FakeInput::default(), out.clone());
        let player = |owner| NetCharacter {
            owner,
            name: "x".into(),
        };
        let a = app.world_mut().spawn(player(Some(5))).id();
        let b = app.world_mut().spawn(player(Some(6))).id();
        app.world_mut().resource_mut::<VoiceMutes>().set(5, true);
        let mut seq = 0;
        let mut say = |app: &mut App, who: Entity| {
            app.world_mut().write_message(VoiceRelay {
                speaker: who,
                seq,
                frames: packets(4),
            });
            seq += 4;
            app.update();
        };
        say(&mut app, a);
        say(&mut app, b);
        assert_eq!(out.samples(a), 0, "muted");
        assert_eq!(out.samples(b), 4 * FRAME);
        app.world_mut().resource_mut::<VoiceMutes>().set(5, false);
        say(&mut app, a);
        assert_eq!(out.samples(a), 4 * FRAME, "heard once unmuted");
        assert_eq!(Sent::frames(&app), 0, "nothing sent without talking");
    }

    /// Push `seconds` of a tone (or of silence: `hz` 0) a frame per update,
    /// as a microphone in real time.
    fn speak(app: &mut App, mic: &FakeInput, hz: f32, seconds: f32) {
        for _ in 0..(seconds * RATE as f32 / FRAME as f32).round() as usize {
            if hz > 0.0 {
                mic.push_tone(hz, FRAME as f32 / RATE as f32);
            } else {
                mic.0.lock().unwrap().extend(std::iter::repeat_n(0i16, FRAME));
            }
            app.update();
        }
    }

    /// The open microphone sends nothing in silence, what is louder than
    /// `voice_threshold` and `VOX_HANG` after it, then stops; a quiet
    /// tone stays under the threshold; with `voice_vox 0` nothing goes
    /// without the key.
    #[test]
    fn the_open_microphone_gates_on_loudness() {
        let mic = FakeInput::default();
        let mut app = app(mic.clone(), FakeOutput::default());
        app.world_mut().resource_mut::<VoiceSettings>().vox = 1;
        app.update();
        speak(&mut app, &mic, 0.0, 0.4);
        assert_eq!(Sent::frames(&app), 0, "silence");
        assert!(!app.world().resource::<VoiceActivity>().local);
        // 0.2 s of a tone (peak 8000, over 4000): ten frames, and the hang.
        speak(&mut app, &mic, 300.0, 0.2);
        assert!(app.world().resource::<VoiceActivity>().local, "talking");
        speak(&mut app, &mic, 0.0, 1.0);
        let hang = (VOX_HANG * RATE as f32 / FRAME as f32).round() as usize;
        assert_eq!(Sent::frames(&app), 10 + hang - 1, "the tone and the hang");
        assert!(!app.world().resource::<VoiceActivity>().local, "quiet again");
        // A tone under the threshold: nothing.
        app.world_mut().resource_mut::<VoiceSettings>().threshold = 9000.0;
        let before = Sent::frames(&app);
        speak(&mut app, &mic, 300.0, 0.3);
        assert_eq!(Sent::frames(&app), before, "under voice_threshold");
        // voice_vox 0: the microphone closes; only the key talks.
        app.world_mut().resource_mut::<VoiceSettings>().vox = 0;
        speak(&mut app, &mic, 300.0, 0.3);
        assert_eq!(Sent::frames(&app), before, "no key");
        app.world_mut().resource_mut::<VoiceRecord>().0 = true;
        speak(&mut app, &mic, 300.0, 0.2);
        app.world_mut().resource_mut::<VoiceRecord>().0 = false;
        speak(&mut app, &mic, 0.0, 0.2);
        let n = Sent::frames(&app) - before;
        assert!((10..=11).contains(&n), "the key's 0.2 s: {n} frames");
    }

    /// The microphone test plays one's voice back and sends nothing; the
    /// level follows the microphone (boosted with `voice_boost`) and is 0
    /// once the test stops.
    #[test]
    fn the_microphone_test_plays_back_and_meters() {
        let mic = FakeInput::default();
        let out = FakeOutput::default();
        let mut app = app(mic.clone(), out.clone());
        app.world_mut().resource_mut::<crate::console::Console>().submit("voice_test");
        app.update();
        assert!(app.world().resource::<VoiceTest>().0, "voice_test toggles it on");
        speak(&mut app, &mic, 300.0, 0.2);
        assert_eq!(Sent::frames(&app), 0, "nothing sent");
        assert_eq!(out.samples(Entity::PLACEHOLDER), 10 * FRAME, "heard back");
        let level = app.world().resource::<VoiceLevel>().0;
        // The tone's peak, 8000 of 32768.
        assert!((level - 8000.0 / 32768.0).abs() < 0.01, "{level}");
        app.world_mut().resource_mut::<VoiceSettings>().boost = 1;
        speak(&mut app, &mic, 300.0, 0.06);
        let boosted = app.world().resource::<VoiceLevel>().0;
        assert!((boosted - BOOST * level).abs() < 0.02, "{boosted}");
        app.world_mut().resource_mut::<crate::console::Console>().submit("voice_test 0");
        app.update();
        speak(&mut app, &mic, 300.0, 0.1);
        assert_eq!(app.world().resource::<VoiceLevel>().0, 0.0, "closed");
        assert_eq!(out.samples(Entity::PLACEHOLDER), 13 * FRAME, "no more played back");
    }

    #[test]
    fn resampling_keeps_the_rate() {
        let mut r = Resampler::new(44_100, 48_000);
        let mut out = 0;
        for i in 0..44_100 {
            r.push((i as f32 * 0.01).sin(), |_| out += 1);
        }
        assert!((out as i32 - 48_000).abs() <= 1, "{out}");
        let mut r = Resampler::new(48_000, 16_000);
        let mut out = Vec::new();
        for i in 0..48 {
            r.push(i as f32, |y| out.push(y));
        }
        assert_eq!(out.len(), 16);
        // Every third input (a ramp: the value is the position).
        assert_eq!(out[2], 5.0);
    }
}
