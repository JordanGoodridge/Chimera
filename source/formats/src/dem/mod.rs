//! Source engine demos (`.dem`, demo protocol 3, network protocol 24: CS:S
//! and the other Orange Box games): everything the server sent to clients,
//! tick by tick. Written from the public descriptions of the format and
//! with the MIT/Apache-2.0 `tf-demo-parser` crate (TF2, same engine
//! branch) as the reference for message layouts; no game code.
//!
//! [`DemoReader`] walks the frames and yields [`Item`]s (a tick and a
//! [`Record`]) while keeping the client-side state in [`DemoState`]: string
//! tables, game event descriptors, send tables flattened per server class,
//! and every entity's current prop values. A record that touches state
//! (entities, string tables) is yielded after the state is updated, so the
//! caller can read new values from `reader.state()`.
//!
//! Frames: signon and packet (net messages), synctick, consolecmd, usercmd
//! (skipped), datatables, stringtables, stop. Net messages decoded:
//! net_Tick, net_SetConVar, net_StringCmd, net_SignonState, svc_Print,
//! svc_ServerInfo, svc_ClassInfo, svc_Create/UpdateStringTable,
//! svc_GameEventList, svc_GameEvent, svc_UserMessage (CS:S names and simple
//! layouts, `events.rs`), svc_Sounds, svc_PacketEntities, svc_TempEntities,
//! svc_SetView; the rest (voice, decals, entity messages, menus, ...) are
//! read past. Compressed string tables are inflated (LZSS or Snappy, both
//! small readers in `stringtable.rs`).
//!
//! `playback`: a demo read once into frames by tick through a game's
//! `Mapper` (its entity values and events), for playing it back.
//!
//! Known shortcuts:
//! - `svc_Sounds`' delta layout varies by branch (flag width, a special-DSP
//!   field): the reader tries the known layouts and keeps the first one
//!   whose sounds fill the message exactly.
//! - SourceTV records one frame per snapshot (`tv_snapshotrate`, default
//!   16 per second), so entity values are seen every ~4 ticks, not each.
//! - A message that fails to parse ends its packet: the rest of that packet
//!   is skipped and an [`Record::Error`] says why.

pub mod bits;
pub mod entities;
pub mod events;
pub mod playback;
pub mod sendtable;
pub mod stringtable;

use bits::{BitReader, Result};
use entities::{Baselines, Entity, EntityChange, MAX_EDICTS, TempEntity};
use events::{EventDescriptor, GameEvent, SOUND_LAYOUTS, Sound, SoundLayout, UserMessage};
use sendtable::{FlatProp, SendTable, ServerClass};
use std::collections::VecDeque;
use stringtable::StringTable;

#[derive(Clone, Debug)]
pub struct Header {
    pub demo_protocol: i32,
    pub network_protocol: i32,
    pub server_name: String,
    pub client_name: String,
    pub map: String,
    pub game_dir: String,
    pub playback_time: f32,
    pub ticks: i32,
    pub frames: i32,
    pub signon_length: i32,
}

pub const HEADER_LEN: usize = 1072;

pub fn read_header(data: &[u8]) -> Result<Header> {
    if data.len() < HEADER_LEN || &data[0..8] != b"HL2DEMO\0" {
        return Err("not a Source demo (no HL2DEMO header)".into());
    }
    let mut r = BitReader::new(&data[8..HEADER_LEN]);
    Ok(Header {
        demo_protocol: r.i32()?,
        network_protocol: r.i32()?,
        server_name: r.fixed_string(260)?,
        client_name: r.fixed_string(260)?,
        map: r.fixed_string(260)?,
        game_dir: r.fixed_string(260)?,
        playback_time: r.f32()?,
        ticks: r.i32()?,
        frames: r.i32()?,
        signon_length: r.i32()?,
    })
}

#[derive(Clone, Debug, Default)]
pub struct ServerInfo {
    pub protocol: u16,
    pub server_count: u32,
    pub hltv: bool,
    pub dedicated: bool,
    pub max_classes: u16,
    pub player_slot: u8,
    pub max_clients: u8,
    pub tick_interval: f32,
    pub os: char,
    pub game_dir: String,
    pub map: String,
    pub skybox: String,
    pub host_name: String,
}

/// What one frame or net message said.
#[derive(Clone, Debug)]
pub enum Record {
    ServerInfo(ServerInfo),
    /// net_Tick: the server's own tick number.
    NetTick(u32),
    ConsoleCmd(String),
    StringCmd(String),
    Print(String),
    SetConVar(Vec<(String, String)>),
    SignonState(u8),
    /// The send tables and server classes arrived (`DemoState::classes`).
    DataTables,
    /// A string table was created or changed: its index and changed entries.
    StringTable {
        table: usize,
        changed: Vec<usize>,
    },
    GameEventList,
    GameEvent(GameEvent),
    UserMessage(UserMessage),
    Sounds(Vec<Sound>),
    Entities(Vec<EntityChange>),
    TempEntities(Vec<TempEntity>),
    SetView(u32),
    /// A message only read past (its name).
    Skipped(&'static str),
    /// A message or frame that failed to parse; the rest of its packet is
    /// skipped.
    Error(String),
    Stop,
}

#[derive(Clone, Debug)]
pub struct Item {
    /// The demo frame's tick (0 at the start of recording).
    pub tick: i32,
    pub record: Record,
}

/// What a client knows at the current point of the demo.
pub struct DemoState {
    pub header: Header,
    pub protocol: u32,
    pub server_info: Option<ServerInfo>,
    pub send_tables: Vec<SendTable>,
    pub classes: Vec<ServerClass>,
    /// Flattened props per class id.
    pub flat: Vec<Vec<FlatProp>>,
    pub class_bits: u32,
    pub string_tables: Vec<StringTable>,
    pub event_list: Vec<EventDescriptor>,
    pub entities: Vec<Option<Entity>>,
    pub baselines: Baselines,
    pub sound_layout: Option<SoundLayout>,
    /// Set by `set_sound_layout`: no detection.
    pub sound_layout_fixed: bool,
    pub server_tick: u32,
}

impl DemoState {
    pub fn table(&self, name: &str) -> Option<&StringTable> {
        self.string_tables.iter().find(|t| t.name == name)
    }

    /// A string table entry's string, e.g. a sound name from `soundprecache`.
    pub fn table_string(&self, table: &str, index: usize) -> Option<&str> {
        self.table(table)?.get(index).map(|e| e.string.as_str())
    }

    pub fn class_name(&self, class: u16) -> &str {
        self.classes.get(class as usize).map(|c| c.name.as_str()).unwrap_or("?")
    }

    pub fn entity(&self, index: u32) -> Option<&Entity> {
        self.entities.get(index as usize)?.as_ref()
    }

    /// The flattened props of an entity's class.
    pub fn entity_props(&self, index: u32) -> Option<&[FlatProp]> {
        let e = self.entity(index)?;
        self.flat.get(e.class as usize).map(|v| v.as_slice())
    }

    /// Seconds per tick (from svc_ServerInfo; 0.015 until it arrives).
    pub fn tick_interval(&self) -> f32 {
        self.server_info.as_ref().map(|s| s.tick_interval).unwrap_or(0.015)
    }

    /// Players from the `userinfo` table: (entity index, name, user id, bot).
    pub fn players(&self) -> Vec<(u32, String, i32, bool)> {
        let Some(t) = self.table("userinfo") else {
            return Vec::new();
        };
        t.entries
            .iter()
            .enumerate()
            .filter_map(|(i, e)| {
                let (name, uid, _guid, fake) = stringtable::parse_userinfo(&e.data)?;
                Some((i as u32 + 1, name, uid, fake))
            })
            .collect()
    }
}

/// Walks a demo's frames. `next` yields items until the stop frame or the
/// end of the data.
pub struct DemoReader<'a> {
    data: &'a [u8],
    pos: usize,
    state: DemoState,
    queue: VecDeque<Item>,
    done: bool,
}

impl<'a> DemoReader<'a> {
    pub fn new(data: &'a [u8]) -> Result<Self> {
        let header = read_header(data)?;
        if header.demo_protocol != 3 {
            return Err(format!("demo protocol {} (only 3 is read)", header.demo_protocol));
        }
        let protocol = header.network_protocol as u32;
        Ok(DemoReader {
            data,
            pos: HEADER_LEN,
            state: DemoState {
                header,
                protocol,
                server_info: None,
                send_tables: Vec::new(),
                classes: Vec::new(),
                flat: Vec::new(),
                class_bits: 0,
                string_tables: Vec::new(),
                event_list: Vec::new(),
                entities: vec![None; MAX_EDICTS],
                baselines: Baselines::default(),
                sound_layout: None,
                sound_layout_fixed: false,
                server_tick: 0,
            },
            queue: VecDeque::new(),
            done: false,
        })
    }

    /// Forces the `svc_Sounds` delta layout instead of detecting it.
    pub fn set_sound_layout(&mut self, layout: SoundLayout) {
        self.state.sound_layout = Some(layout);
        self.state.sound_layout_fixed = true;
    }

    pub fn state(&self) -> &DemoState {
        &self.state
    }

    pub fn header(&self) -> &Header {
        &self.state.header
    }

    fn take(&mut self, n: usize) -> Result<&'a [u8]> {
        let end = self
            .pos
            .checked_add(n)
            .filter(|&e| e <= self.data.len())
            .ok_or("demo ends inside a frame")?;
        let s = &self.data[self.pos..end];
        self.pos = end;
        Ok(s)
    }

    fn i32(&mut self) -> Result<i32> {
        Ok(i32::from_le_bytes(self.take(4)?.try_into().unwrap()))
    }

    fn sized(&mut self) -> Result<&'a [u8]> {
        let n = self.i32()?;
        if n < 0 {
            return Err(format!("negative frame length {n}"));
        }
        self.take(n as usize)
    }

    /// Reads one frame into the queue.
    fn frame(&mut self) -> Result<()> {
        if self.pos >= self.data.len() {
            self.done = true;
            return Ok(());
        }
        let cmd = self.take(1)?[0];
        if cmd == 7 {
            self.done = true;
            let tick = if self.data.len() >= self.pos + 4 {
                self.i32()?
            } else {
                -1
            };
            self.queue.push_back(Item {
                tick,
                record: Record::Stop,
            });
            return Ok(());
        }
        let tick = self.i32()?;
        // Signon frames carry the server's tick; the recording starts at 0.
        let tick = if cmd == 1 { 0 } else { tick };
        let push = |q: &mut VecDeque<Item>, record| q.push_back(Item { tick, record });
        match cmd {
            1 | 2 => {
                self.take(76 + 8)?; // view info (flags, 2 x origin/angles/local angles), sequences
                let data = self.sized()?;
                let mut r = BitReader::new(data);
                let mut out = Vec::new();
                if let Err(e) = read_messages(&mut self.state, &mut r, &mut out) {
                    out.push(Record::Error(e));
                }
                for rec in out {
                    push(&mut self.queue, rec);
                }
            }
            3 => {}
            4 => {
                let data = self.sized()?;
                let s = BitReader::new(data).string()?;
                push(&mut self.queue, Record::ConsoleCmd(s));
            }
            5 => {
                self.i32()?;
                self.sized()?;
            }
            6 => {
                let data = self.sized()?;
                let (tables, classes) = sendtable::read_data_tables(&mut BitReader::new(data))?;
                let mut flat = Vec::with_capacity(classes.len());
                for c in &classes {
                    flat.push(sendtable::flatten(&tables, &c.table)?);
                }
                self.state.class_bits = bits::bits_for(classes.len() as u32);
                self.state.send_tables = tables;
                self.state.classes = classes;
                self.state.flat = flat;
                self.state.baselines.statics.clear();
                push(&mut self.queue, Record::DataTables);
            }
            8 => {
                let data = self.sized()?;
                let tables = stringtable::read_snapshot(&mut BitReader::new(data))?;
                for (name, entries) in tables {
                    let Some(ti) = self.state.string_tables.iter().position(|t| t.name == name) else {
                        continue;
                    };
                    let changed: Vec<usize> = (0..entries.len()).collect();
                    self.state.string_tables[ti].entries = entries;
                    if name == "instancebaseline" {
                        self.state.baselines.statics.clear();
                    }
                    push(&mut self.queue, Record::StringTable { table: ti, changed });
                }
            }
            other => return Err(format!("unknown demo frame type {other} at byte {}", self.pos - 5)),
        }
        Ok(())
    }

    pub fn next_item(&mut self) -> Option<Result<Item>> {
        while self.queue.is_empty() {
            if self.done {
                return None;
            }
            if let Err(e) = self.frame() {
                self.done = true;
                return Some(Err(e));
            }
        }
        self.queue.pop_front().map(Ok)
    }
}

const MESSAGE_NAMES: [&str; 33] = [
    "net_NOP",
    "net_Disconnect",
    "net_File",
    "net_Tick",
    "net_StringCmd",
    "net_SetConVar",
    "net_SignonState",
    "svc_Print",
    "svc_ServerInfo",
    "svc_SendTable",
    "svc_ClassInfo",
    "svc_SetPause",
    "svc_CreateStringTable",
    "svc_UpdateStringTable",
    "svc_VoiceInit",
    "svc_VoiceData",
    "svc_HLTV",
    "svc_Sounds",
    "svc_SetView",
    "svc_FixAngle",
    "svc_CrosshairAngle",
    "svc_BSPDecal",
    "svc_TerrainMod",
    "svc_UserMessage",
    "svc_EntityMessage",
    "svc_GameEvent",
    "svc_PacketEntities",
    "svc_TempEntities",
    "svc_Prefetch",
    "svc_Menu",
    "svc_GameEventList",
    "svc_GetCvarValue",
    "svc_CmdKeyValues",
];

/// The net messages of one packet (6-bit type, then the body).
fn read_messages(st: &mut DemoState, r: &mut BitReader, out: &mut Vec<Record>) -> Result<()> {
    while r.bits_left() > 6 {
        let t = r.bits(6)? as usize;
        let name = MESSAGE_NAMES.get(t).copied().unwrap_or("?");
        let rec = read_message(st, t, r).map_err(|e| format!("{name}: {e}"))?;
        out.push(rec);
    }
    Ok(())
}

fn read_message(st: &mut DemoState, t: usize, r: &mut BitReader) -> Result<Record> {
    let skipped = |n: &'static str| Ok(Record::Skipped(n));
    match t {
        0 => skipped("net_NOP"),
        1 => Ok(Record::Print(format!("disconnect: {}", r.string()?))),
        2 => {
            r.u32()?;
            r.string()?;
            r.bit()?;
            skipped("net_File")
        }
        3 => {
            let tick = r.u32()?;
            r.u16()?;
            r.u16()?;
            st.server_tick = tick;
            Ok(Record::NetTick(tick))
        }
        4 => Ok(Record::StringCmd(r.string()?)),
        5 => {
            let n = r.u8()?;
            let mut vars = Vec::with_capacity(n as usize);
            for _ in 0..n {
                vars.push((r.string()?, r.string()?));
            }
            Ok(Record::SetConVar(vars))
        }
        6 => {
            let state = r.u8()?;
            r.i32()?;
            Ok(Record::SignonState(state))
        }
        7 => Ok(Record::Print(r.string()?)),
        8 => {
            let protocol = r.u16()?;
            let server_count = r.u32()?;
            let hltv = r.bit()?;
            let dedicated = r.bit()?;
            r.u32()?; // client.dll CRC
            let max_classes = r.u16()?;
            if protocol > 17 {
                r.skip(128)?; // map MD5
            } else {
                r.u32()?;
            }
            let player_slot = r.u8()?;
            let max_clients = r.u8()?;
            let tick_interval = r.f32()?;
            let os = r.u8()? as char;
            let info = ServerInfo {
                protocol,
                server_count,
                hltv,
                dedicated,
                max_classes,
                player_slot,
                max_clients,
                tick_interval,
                os,
                game_dir: r.string()?,
                map: r.string()?,
                skybox: r.string()?,
                host_name: r.string()?,
            };
            if protocol > 15 {
                r.bit()?; // replay
            }
            st.server_info = Some(info.clone());
            Ok(Record::ServerInfo(info))
        }
        9 => {
            r.bit()?;
            let n = r.u16()? as usize;
            r.skip(n)?;
            skipped("svc_SendTable")
        }
        10 => {
            let count = r.u16()? as u32;
            let create_on_client = r.bit()?;
            if !create_on_client {
                let bits = bits::bits_for(count);
                for _ in 0..count {
                    r.bits(bits)?;
                    r.string()?;
                    r.string()?;
                }
            }
            skipped("svc_ClassInfo")
        }
        11 => {
            r.bit()?;
            skipped("svc_SetPause")
        }
        12 => {
            let (table, changed) = stringtable::read_create(r, st.protocol)?;
            if table.name == "instancebaseline" {
                st.baselines.statics.clear();
            }
            st.string_tables.push(table);
            Ok(Record::StringTable {
                table: st.string_tables.len() - 1,
                changed,
            })
        }
        13 => {
            let (id, count, mut body) = stringtable::read_update_header(r)?;
            let table = st
                .string_tables
                .get_mut(id)
                .ok_or_else(|| format!("no string table {id}"))?;
            let changed = stringtable::apply_update(&mut body, table, count)?;
            if table.name == "instancebaseline" {
                st.baselines.statics.clear();
            }
            Ok(Record::StringTable { table: id, changed })
        }
        14 => {
            r.string()?;
            let quality = r.u8()?;
            if quality == 255 {
                r.u16()?;
            }
            skipped("svc_VoiceInit")
        }
        15 => {
            r.u8()?;
            r.u8()?;
            let n = r.u16()? as usize;
            r.skip(n)?;
            skipped("svc_VoiceData")
        }
        17 => {
            let reliable = r.bit()?;
            let (count, bits) = if reliable {
                (1, r.u8()? as usize)
            } else {
                (r.u8()? as u32, r.u16()? as usize)
            };
            let body = r.sub(bits)?;
            let try_layout = |l: SoundLayout| events::read_sounds(&body, count, l);
            if let Some(l) = st.sound_layout {
                match try_layout(l) {
                    Ok(s) => return Ok(Record::Sounds(s)),
                    Err(e) if st.sound_layout_fixed => return Ok(Record::Error(format!("svc_Sounds: {e}"))),
                    Err(_) => {}
                }
            }
            let mut last_err = String::new();
            for &l in SOUND_LAYOUTS {
                match try_layout(l) {
                    Ok(s) => {
                        // Only settle on a layout from a message with more than one sound.
                        if count > 1 {
                            st.sound_layout = Some(l);
                        }
                        return Ok(Record::Sounds(s));
                    }
                    Err(e) => last_err = e,
                }
            }
            Ok(Record::Error(format!("svc_Sounds: no known layout fits ({last_err})")))
        }
        18 => Ok(Record::SetView(r.bits(11)?)),
        19 => {
            r.skip(1 + 48)?;
            skipped("svc_FixAngle")
        }
        20 => {
            r.skip(48)?;
            skipped("svc_CrosshairAngle")
        }
        21 => {
            r.bit_vec_coord()?;
            r.bits(9)?;
            if r.bit()? {
                r.bits(11)?;
                r.bits(13)?;
            }
            r.bit()?;
            skipped("svc_BSPDecal")
        }
        23 => Ok(Record::UserMessage(events::read_user_message(r)?)),
        24 => {
            r.bits(11)?;
            r.bits(9)?;
            let n = r.bits(11)? as usize;
            r.skip(n)?;
            skipped("svc_EntityMessage")
        }
        25 => {
            let n = r.bits(11)? as usize;
            let mut body = r.sub(n)?;
            if st.event_list.is_empty() {
                return skipped("svc_GameEvent (before the event list)");
            }
            Ok(Record::GameEvent(events::read_event(&mut body, &st.event_list)?))
        }
        26 => entities::read_packet_entities(st, r),
        27 => entities::read_temp_entities(st, r),
        28 => {
            r.bits(if st.protocol > 22 { 14 } else { 13 })?;
            skipped("svc_Prefetch")
        }
        29 => {
            r.u16()?;
            let n = r.u16()? as usize;
            r.skip(n * 8)?;
            skipped("svc_Menu")
        }
        30 => {
            st.event_list = events::read_event_list(r)?;
            Ok(Record::GameEventList)
        }
        31 => {
            r.i32()?;
            r.string()?;
            skipped("svc_GetCvarValue")
        }
        32 => {
            let n = r.u32()? as usize;
            r.skip(n * 8)?;
            skipped("svc_CmdKeyValues")
        }
        other => Err(format!("unknown net message type {other}")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_non_demos() {
        assert!(DemoReader::new(b"not a demo").is_err());
    }

    /// The sample demos (recorded on a CS:S dedicated server with SourceTV;
    /// never committed): `$MASHUP_DEMOS` or the workspace's `target/demos/`.
    /// Skipped when there are none.
    #[test]
    fn parses_sample_demos() {
        let dir = std::env::var_os("MASHUP_DEMOS")
            .map(std::path::PathBuf::from)
            .unwrap_or_else(|| std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/demos"));
        let Ok(list) = std::fs::read_dir(&dir) else {
            eprintln!("skipping: no sample demos in {}", dir.display());
            return;
        };
        let mut any = false;
        for path in list
            .flatten()
            .map(|e| e.path())
            .filter(|p| p.extension().is_some_and(|e| e == "dem"))
        {
            any = true;
            let data = std::fs::read(&path).unwrap();
            let mut reader = DemoReader::new(&data).unwrap();
            let (mut events, mut entity_updates, mut sounds, mut errors) = (0, 0, 0, Vec::new());
            let mut last_tick = 0;
            while let Some(item) = reader.next_item() {
                let item = item.unwrap_or_else(|e| panic!("{}: {e}", path.display()));
                last_tick = last_tick.max(item.tick);
                match item.record {
                    Record::GameEvent(_) => events += 1,
                    Record::Entities(c) => entity_updates += c.len(),
                    Record::Sounds(s) => sounds += s.len(),
                    Record::Error(e) => errors.push(e),
                    _ => {}
                }
            }
            let st = reader.state();
            assert!(
                errors.is_empty(),
                "{}: {} errors, first: {}",
                path.display(),
                errors.len(),
                errors[0]
            );
            assert!(!st.classes.is_empty() && st.table("soundprecache").is_some());
            // An idle recording may have no sounds; every one has events
            // (hltv_status) and entity updates.
            assert!(events > 0 && entity_updates > 0, "{}", path.display());
            eprintln!(
                "{}: {events} events, {entity_updates} entity updates, {sounds} sounds",
                path.display()
            );
            assert!(last_tick > 0);
            let players = st
                .entities
                .iter()
                .flatten()
                .filter(|e| st.class_name(e.class) == "CCSPlayer")
                .count();
            assert!(players > 0, "{}: no CCSPlayer entities", path.display());
        }
        if !any {
            eprintln!("skipping: no .dem files in {}", dir.display());
        }
    }
}
