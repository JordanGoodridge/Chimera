//! Game events (`svc_GameEventList` describes them, `svc_GameEvent`
//! carries one), user messages (`svc_UserMessage`; CS:S's ids and the
//! layouts of the simple ones) and sounds (`svc_Sounds`).

use super::bits::{BitReader, Result};
use std::fmt;

// ---- game events ---------------------------------------------------------

#[derive(Clone, Debug)]
pub struct EventDescriptor {
    pub id: u32,
    pub name: String,
    /// (key, value type: 1 string, 2 float, 3 long, 4 short, 5 byte,
    /// 6 bool, 7 local/not networked).
    pub keys: Vec<(String, u8)>,
}

#[derive(Clone, Debug, PartialEq)]
pub enum EventValue {
    String(String),
    Float(f32),
    Long(i32),
    Short(i16),
    Byte(u8),
    Bool(bool),
    Local,
}

impl fmt::Display for EventValue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            EventValue::String(s) => write!(f, "{s:?}"),
            EventValue::Float(v) => write!(f, "{v}"),
            EventValue::Long(v) => write!(f, "{v}"),
            EventValue::Short(v) => write!(f, "{v}"),
            EventValue::Byte(v) => write!(f, "{v}"),
            EventValue::Bool(v) => write!(f, "{}", *v as u8),
            EventValue::Local => write!(f, "-"),
        }
    }
}

#[derive(Clone, Debug)]
pub struct GameEvent {
    pub name: String,
    pub fields: Vec<(String, EventValue)>,
}

impl GameEvent {
    pub fn get(&self, key: &str) -> Option<&EventValue> {
        self.fields.iter().find(|(k, _)| k == key).map(|(_, v)| v)
    }
}

impl fmt::Display for GameEvent {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.name)?;
        for (k, v) in &self.fields {
            write!(f, " {k}={v}")?;
        }
        Ok(())
    }
}

pub fn read_event_list(r: &mut BitReader) -> Result<Vec<EventDescriptor>> {
    let count = r.bits(9)?;
    let length = r.bits(20)? as usize;
    let mut body = r.sub(length)?;
    let mut out = Vec::with_capacity(count as usize);
    for _ in 0..count {
        let id = body.bits(9)?;
        let name = body.string()?;
        let mut keys = Vec::new();
        loop {
            let ty = body.bits(3)? as u8;
            if ty == 0 {
                break;
            }
            keys.push((body.string()?, ty));
        }
        out.push(EventDescriptor { id, name, keys });
    }
    Ok(out)
}

/// Decodes an event body (after its 11-bit length) against the list.
pub fn read_event(r: &mut BitReader, list: &[EventDescriptor]) -> Result<GameEvent> {
    let id = r.bits(9)?;
    let d = list
        .iter()
        .find(|d| d.id == id)
        .ok_or_else(|| format!("game event id {id} not in the event list"))?;
    let mut fields = Vec::with_capacity(d.keys.len());
    for (key, ty) in &d.keys {
        let v = match ty {
            1 => EventValue::String(r.string()?),
            2 => EventValue::Float(r.f32()?),
            3 => EventValue::Long(r.i32()?),
            4 => EventValue::Short(r.i16()?),
            5 => EventValue::Byte(r.u8()?),
            6 => EventValue::Bool(r.bit()?),
            _ => EventValue::Local,
        };
        fields.push((key.clone(), v));
    }
    Ok(GameEvent {
        name: d.name.clone(),
        fields,
    })
}

// ---- user messages -------------------------------------------------------

/// CS:S's user message names by id (registration order; ids are not in
/// the demo). Checked against the sample demos' contents (TextMsg carries
/// `#Round_*`/`#Game_*` keys, HintText `#Hint_*`, SendAudio `Radio.*`).
pub const CSS_USER_MESSAGES: &[&str] = &[
    "Geiger",
    "Train",
    "HudText",
    "SayText",
    "SayText2",
    "TextMsg",
    "HudMsg",
    "ResetHUD",
    "GameTitle",
    "ItemPickup",
    "ShowMenu",
    "Shake",
    "Fade",
    "VGUIMenu",
    "Rumble",
    "CloseCaption",
    "SendAudio",
    "RawAudio",
    "VoiceMask",
    "RequestState",
    "BarTime",
    "Damage",
    "RadioText",
    "HintText",
    "KeyHintText",
    "ReloadEffect",
    "PlayerAnimEvent",
    "AmmoDenied",
    "UpdateRadar",
    "KillCam",
    "MarkAchievement",
    "CallVoteFailed",
    "VoteStart",
    "VotePass",
    "VoteFailed",
    "VoteSetup",
    "SPHapWeapEvent",
    "HapDmg",
    "HapPunch",
    "HapSetDrag",
    "HapSetConst",
    "HapMeleeContact",
    "PlayerStatsUpdate_DEPRECATED",
    "AchievementEvent",
    "MatchEndConditions",
    "MatchStatsUpdate",
    "PlayerStatsUpdate",
];

pub fn user_message_name(id: u8) -> String {
    CSS_USER_MESSAGES
        .get(id as usize)
        .map(|s| s.to_string())
        .unwrap_or_else(|| format!("UserMessage{id}"))
}

#[derive(Clone, Debug)]
pub struct UserMessage {
    pub id: u8,
    pub name: String,
    /// Decoded fields for the layouts known here; empty otherwise.
    pub fields: Vec<(String, String)>,
    /// The message body.
    pub data: Vec<u8>,
    pub bits: usize,
}

impl fmt::Display for UserMessage {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.name)?;
        if self.fields.is_empty() {
            write!(f, " ({} bits)", self.bits)?;
            for b in self.data.iter().take(48) {
                write!(f, " {b:02x}")?;
            }
            return Ok(());
        }
        for (k, v) in &self.fields {
            write!(f, " {k}={v}")?;
        }
        Ok(())
    }
}

fn decode_user_message(name: &str, r: &mut BitReader) -> Result<Vec<(String, String)>> {
    let mut f: Vec<(String, String)> = Vec::new();
    macro_rules! put {
        ($k:expr, $v:expr) => {
            f.push(($k.to_string(), $v.to_string()))
        };
    }
    let rest_strings = |r: &mut BitReader, f: &mut Vec<(String, String)>, label: &str| -> Result<()> {
        let mut i = 1;
        while r.bits_left() >= 8 {
            let s = r.string()?;
            f.push((format!("{label}{i}"), format!("{s:?}")));
            i += 1;
        }
        Ok(())
    };
    match name {
        "TextMsg" => {
            put!("dest", r.u8()?);
            put!("msg", format!("{:?}", r.string()?));
            rest_strings(r, &mut f, "p")?;
        }
        "SayText" => {
            put!("client", r.u8()?);
            put!("text", format!("{:?}", r.string()?));
            put!("chat", r.u8()?);
        }
        "SayText2" | "RadioText" => {
            if name == "RadioText" {
                put!("dest", r.u8()?);
            }
            put!("client", r.u8()?);
            if name == "SayText2" {
                put!("chat", r.u8()?);
            }
            put!("msg", format!("{:?}", r.string()?));
            rest_strings(r, &mut f, "p")?;
        }
        "HintText" | "ItemPickup" | "SendAudio" | "HudText" | "GameTitle" => {
            put!("text", format!("{:?}", r.string()?));
        }
        "KeyHintText" => {
            let n = r.u8()?;
            put!("count", n);
            for i in 0..n {
                put!(format!("text{}", i + 1), format!("{:?}", r.string()?));
            }
        }
        "HudMsg" => {
            put!("channel", r.u8()?);
            put!("x", r.f32()?);
            put!("y", r.f32()?);
            let c1 = [r.u8()?, r.u8()?, r.u8()?, r.u8()?];
            let c2 = [r.u8()?, r.u8()?, r.u8()?, r.u8()?];
            put!("color1", format!("{c1:?}"));
            put!("color2", format!("{c2:?}"));
            put!("effect", r.u8()?);
            put!("fadein", r.f32()?);
            put!("fadeout", r.f32()?);
            put!("hold", r.f32()?);
            put!("fxtime", r.f32()?);
            put!("text", format!("{:?}", r.string()?));
        }
        "Fade" => {
            // Durations are 1/512 s fixed point (SCREENFADE_FRACBITS = 9).
            let duration = r.u16()?;
            let hold = r.u16()?;
            put!("duration", format!("{} ({}s)", duration, duration as f32 / 512.0));
            put!("hold", format!("{} ({}s)", hold, hold as f32 / 512.0));
            put!("flags", format!("{:#x}", r.u16()?));
            put!("rgba", format!("{:?}", [r.u8()?, r.u8()?, r.u8()?, r.u8()?]));
        }
        "Shake" => {
            put!("command", r.u8()?);
            put!("amplitude", r.f32()?);
            put!("frequency", r.f32()?);
            put!("duration", r.f32()?);
        }
        "VGUIMenu" => {
            put!("name", format!("{:?}", r.string()?));
            put!("show", r.u8()?);
            let n = r.u8()?;
            for _ in 0..n {
                let k = r.string()?;
                let v = r.string()?;
                f.push((k, format!("{v:?}")));
            }
        }
        "ShowMenu" => {
            put!("slots", format!("{:#x}", r.u16()?));
            put!("time", r.u8()? as i8);
            put!("more", r.u8()?);
            put!("text", format!("{:?}", r.string()?));
        }
        "RawAudio" => {
            // Bot chatter: pitch, speaking entity, duration, wav name.
            put!("pitch", r.u8()?);
            put!("entity", r.u8()?);
            put!("duration", r.f32()?);
            put!("file", format!("{:?}", r.string()?));
        }
        "BarTime" => {
            put!("seconds", r.i16()?);
        }
        "ResetHUD" | "RequestState" => {
            if r.bits_left() >= 8 {
                put!("value", r.u8()?);
            }
        }
        "ReloadEffect" => {
            put!("entity", r.u16()?);
        }
        "Damage" => {
            put!("armor", r.u8()?);
            put!("damage", r.u8()?);
            put!("bits", format!("{:#x}", r.i32()?));
            let v = r.bit_vec_coord()?;
            put!("from", format!("({}, {}, {})", v[0], v[1], v[2]));
        }
        _ => {}
    }
    Ok(f)
}

/// `svc_UserMessage`: type, 11-bit length, body.
pub fn read_user_message(r: &mut BitReader) -> Result<UserMessage> {
    let id = r.u8()?;
    let bits = r.bits(11)? as usize;
    let mut body = r.sub(bits)?;
    let data = {
        let mut copy = body.clone();
        let mut out = Vec::with_capacity(bits.div_ceil(8));
        while copy.bits_left() > 0 {
            let n = copy.bits_left().min(8) as u32;
            out.push(copy.bits(n)? as u8);
        }
        out
    };
    let name = user_message_name(id);
    // A layout that does not fit leaves the raw bytes to show instead.
    let fields = decode_user_message(&name, &mut body).unwrap_or_default();
    Ok(UserMessage {
        id,
        name,
        fields,
        data,
        bits,
    })
}

// ---- sounds --------------------------------------------------------------

pub const SND_STOP: u32 = 1 << 2;

/// One started (or stopped) sound as the server sent it.
#[derive(Clone, Debug, PartialEq)]
pub struct Sound {
    pub entity: u32,
    /// Index into `soundprecache` (or the sentence list when `sentence`).
    pub sound_index: u32,
    pub flags: u32,
    pub channel: u32,
    pub ambient: bool,
    pub sentence: bool,
    pub sequence: u32,
    pub volume: f32,
    pub level: u32,
    pub pitch: u32,
    pub delay: f32,
    /// Rounded to 8 units per axis by the encoding.
    pub origin: [f32; 3],
    pub speaker: i32,
}

impl Default for Sound {
    fn default() -> Self {
        Sound {
            entity: 0,
            sound_index: 0,
            flags: 0,
            channel: 6, // CHAN_STATIC
            ambient: false,
            sentence: false,
            sequence: 0,
            volume: 1.0,
            level: 75, // SNDLVL_NORM
            pitch: 100,
            delay: 0.0,
            origin: [0.0; 3],
            speaker: -1,
        }
    }
}

/// The delta layout details that vary across engine branches; the reader
/// picks the one whose sounds fill the message exactly.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SoundLayout {
    pub flag_bits: u32,
    pub special_dsp: bool,
}

pub const SOUND_LAYOUTS: &[SoundLayout] = &[
    // CS:S (network protocol 24), checked on SourceTV demos: decoded
    // positions match the entities making the sounds.
    SoundLayout {
        flag_bits: 11,
        special_dsp: true,
    },
    SoundLayout {
        flag_bits: 11,
        special_dsp: false,
    },
    SoundLayout {
        flag_bits: 9,
        special_dsp: true,
    },
    SoundLayout {
        flag_bits: 9,
        special_dsp: false,
    },
];

fn read_sound(r: &mut BitReader, prev: &Sound, layout: SoundLayout) -> Result<Sound> {
    let mut s = prev.clone();
    if r.bit()? {
        s.entity = if r.bit()? { r.bits(5)? } else { r.bits(11)? };
    }
    if r.bit()? {
        s.sound_index = r.bits(14)?;
    }
    if r.bit()? {
        s.flags = r.bits(layout.flag_bits)?;
    }
    if r.bit()? {
        s.channel = r.bits(3)?;
    }
    s.ambient = r.bit()?;
    s.sentence = r.bit()?;
    if s.flags == SND_STOP {
        // A stop carries no playback fields.
        s.sequence = 0;
        s.volume = 0.0;
        s.level = 0;
        s.pitch = 100;
        s.delay = 0.0;
        s.origin = [0.0; 3];
        s.speaker = -1;
        return Ok(s);
    }
    if r.bit()? {
        // same sequence number
    } else if r.bit()? {
        s.sequence = prev.sequence + 1;
    } else {
        s.sequence = r.bits(10)?;
    }
    if r.bit()? {
        s.volume = r.bits(7)? as f32 / 127.0;
    }
    if r.bit()? {
        s.level = r.bits(9)?;
    }
    if r.bit()? {
        s.pitch = r.bits(8)?;
    }
    if layout.special_dsp && r.bit()? {
        r.bits(8)?;
    }
    if r.bit()? {
        let mut d = r.signed(13)? as f32 / 1000.0;
        if d < 0.0 {
            d *= 10.0;
        }
        s.delay = d - 0.1;
    }
    for i in 0..3 {
        if r.bit()? {
            s.origin[i] = (r.signed(12)? * 8) as f32;
        }
    }
    if r.bit()? {
        s.speaker = r.signed(12)?;
    }
    Ok(s)
}

/// The sounds of one `svc_Sounds` body (`count` delta-coded entries).
pub fn read_sounds(body: &BitReader, count: u32, layout: SoundLayout) -> Result<Vec<Sound>> {
    let mut r = body.clone();
    let mut prev = Sound::default();
    let mut out = Vec::with_capacity(count as usize);
    for _ in 0..count {
        let s = read_sound(&mut r, &prev, layout)?;
        prev = s.clone();
        out.push(s);
    }
    if r.bits_left() != 0 {
        return Err(format!("{} bits left after {count} sounds", r.bits_left()));
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::super::bits::tests::pack;
    use super::*;

    #[test]
    fn event_list_and_event() {
        let s = |t: &str| t.bytes().chain([0]).map(|b| (b as u32, 8)).collect::<Vec<_>>();
        let mut def = vec![(7, 9)];
        def.extend(s("player_death"));
        def.push((4, 3));
        def.extend(s("userid"));
        def.push((1, 3));
        def.extend(s("weapon"));
        def.push((6, 3));
        def.extend(s("headshot"));
        def.push((0, 3));
        let body_bits: u32 = def.iter().map(|(_, n)| n).sum();
        let mut list = vec![(1, 9), (body_bits, 20)];
        list.extend(def);
        let data = pack(&list);
        let events = read_event_list(&mut BitReader::new(&data)).unwrap();
        assert_eq!(events[0].name, "player_death");

        let mut ev = vec![(7, 9), (3, 16)];
        ev.extend(s("ak47"));
        ev.push((1, 1));
        let data = pack(&ev);
        let e = read_event(&mut BitReader::new(&data), &events).unwrap();
        assert_eq!(e.to_string(), "player_death userid=3 weapon=\"ak47\" headshot=1");
    }

    /// A `svc_UserMessage` body: type byte, 11-bit length, the bytes.
    fn user_message(id: u8, body: &[u8]) -> UserMessage {
        let mut f = vec![(id as u32, 8), (body.len() as u32 * 8, 11)];
        f.extend(body.iter().map(|&b| (b as u32, 8)));
        read_user_message(&mut BitReader::new(&pack(&f))).unwrap()
    }

    #[test]
    fn user_message_layouts() {
        let m = user_message(5, b"\x04#Round_Draw\0\0");
        assert_eq!(m.to_string(), "TextMsg dest=4 msg=\"#Round_Draw\" p1=\"\"");
        // Fade: 2 s (1024/512), hold 0.5 s, flags 1, white.
        let m = user_message(12, &[0x00, 0x04, 0x00, 0x01, 0x01, 0x00, 255, 255, 255, 255]);
        assert_eq!(
            m.to_string(),
            "Fade duration=1024 (2s) hold=256 (0.5s) flags=0x1 rgba=[255, 255, 255, 255]"
        );
        let m = user_message(17, b"\x72\x03\x00\x00\x00\x40bot/sniper.wav\0");
        assert_eq!(
            m.to_string(),
            "RawAudio pitch=114 entity=3 duration=2 file=\"bot/sniper.wav\""
        );
    }

    #[test]
    fn sound_delta_chain() {
        // Two sounds: the first sets entity 3 (5-bit form) and sound 42,
        // the second only bumps the sequence number.
        let layout = SoundLayout {
            flag_bits: 11,
            special_dsp: false,
        };
        let first = [
            (1, 1),
            (1, 1),
            (3, 5), // entity
            (1, 1),
            (42, 14), // sound
            (0, 1),   // flags same
            (0, 1),   // channel same
            (0, 1),
            (0, 1), // ambient, sentence
            (0, 1),
            (0, 1),
            (5, 10), // sequence 5
            (0, 1),
            (0, 1),
            (0, 1), // volume, level, pitch
            (0, 1), // delay
            (0, 1),
            (0, 1),
            (0, 1), // origin
            (0, 1), // speaker
        ];
        let second = [
            (0, 1),
            (0, 1),
            (0, 1),
            (0, 1),
            (0, 1),
            (0, 1),
            (0, 1),
            (1, 1), // sequence + 1
            (0, 1),
            (0, 1),
            (0, 1),
            (0, 1),
            (0, 1),
            (0, 1),
            (0, 1),
            (0, 1),
        ];
        let mut f = first.to_vec();
        f.extend(second);
        let bits: u32 = f.iter().map(|(_, n)| n).sum();
        let data = pack(&f);
        let mut r = BitReader::new(&data);
        let body = r.sub(bits as usize).unwrap();
        let sounds = read_sounds(&body, 2, layout).unwrap();
        assert_eq!(
            (sounds[0].entity, sounds[0].sound_index, sounds[0].sequence),
            (3, 42, 5)
        );
        assert_eq!(
            (sounds[1].entity, sounds[1].sound_index, sounds[1].sequence),
            (3, 42, 6)
        );
    }
}
