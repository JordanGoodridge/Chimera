//! Demo playback's plumbing, for any Source game: a demo read once into a
//! [`Timeline`] of frames (one per recorded snapshot, by demo tick), each
//! holding every entity a game cares about as that game's own value and
//! the one-shot things (shots, sounds, messages, game events) that came
//! with it. A game says what it cares about through a [`Mapper`]: which
//! classes and props make its entity values, which records make its
//! events. Playing is then picking frames by tick: the state at a tick is
//! the frame at or before it (or between it and the next, to ease), and
//! seeking anywhere is as cheap as playing on, since every frame holds the
//! whole state (no deltas to replay).
//!
//! Source's conventions every game shares: entity handles (11 bits of
//! index, the rest a serial number; `INVALID_HANDLE` for none,
//! [`handle_index`]), props looked up by path once per class
//! ([`ClassProps`]), typed reads of prop values ([`Props`]), seconds per
//! tick ([`Timeline::seconds`]).
//!
//! Known shortcuts:
//! - A frame is made at each `svc_PacketEntities` (and at a tick with only
//!   one-shot records); a SourceTV demo has one every `tv_snapshotrate`
//!   (16 per second by default, ~4 ticks), so values between are eased by
//!   the player, not recorded.
//! - Frames keep every mapped entity whole: memory grows with frames ×
//!   mapped entities (a 150 s bot round: 2500 frames of ~20 small values).

use super::{
    DemoReader, DemoState, Header, Record,
    bits::Result,
    entities::{ChangeKind, Entity, EntityChange},
    sendtable::{FlatProp, PropValue},
};

/// No entity (an entity handle prop with every bit of its 21 set).
pub const INVALID_HANDLE: i64 = 0x1F_FFFF;
/// Bits of an entity handle that are the entity's index.
pub const HANDLE_INDEX_BITS: u32 = 11;

/// The entity index of a handle prop's value, None for no entity.
pub fn handle_index(handle: i64) -> Option<u32> {
    (handle >= 0 && handle != INVALID_HANDLE).then(|| (handle as u32) & ((1 << HANDLE_INDEX_BITS) - 1))
}

/// A class's props by path, looked up once: `ClassProps::new(flat, &[..])`
/// then `props.at(i)` is the flattened index of the `i`th path (None when
/// the class has no such prop). A path matches a flattened prop's whole
/// path (`csnonlocaldata.m_vecOrigin`, `m_iScore.003`), else its last part
/// (`m_vecOrigin`).
#[derive(Clone, Debug, Default)]
pub struct ClassProps(Vec<Option<usize>>);

impl ClassProps {
    pub fn new(flat: &[FlatProp], paths: &[&str]) -> Self {
        Self(
            paths
                .iter()
                .map(|p| {
                    flat.iter()
                        .position(|f| f.path == *p)
                        .or_else(|| flat.iter().position(|f| f.name() == *p))
                })
                .collect(),
        )
    }

    pub fn at(&self, i: usize) -> Option<usize> {
        self.0.get(i).copied().flatten()
    }
}

/// Typed reads of an entity's current prop values by flattened index
/// (None: no such prop, never sent, or another type).
pub trait Props {
    fn value(&self, prop: Option<usize>) -> Option<&PropValue>;

    fn int(&self, prop: Option<usize>) -> Option<i64> {
        match self.value(prop)? {
            PropValue::Int(v) => Some(*v),
            PropValue::Float(v) => Some(*v as i64),
            _ => None,
        }
    }

    fn float(&self, prop: Option<usize>) -> Option<f32> {
        match self.value(prop)? {
            PropValue::Float(v) => Some(*v),
            PropValue::Int(v) => Some(*v as f32),
            _ => None,
        }
    }

    fn vector(&self, prop: Option<usize>) -> Option<[f32; 3]> {
        match self.value(prop)? {
            PropValue::Vector(v) => Some(*v),
            PropValue::VectorXY([x, y]) => Some([*x, *y, 0.0]),
            _ => None,
        }
    }

    fn string(&self, prop: Option<usize>) -> Option<&str> {
        match self.value(prop)? {
            PropValue::String(s) => Some(s),
            _ => None,
        }
    }

    /// A handle prop's entity index (None: no entity).
    fn handle(&self, prop: Option<usize>) -> Option<u32> {
        handle_index(self.int(prop)?)
    }
}

impl Props for Entity {
    fn value(&self, prop: Option<usize>) -> Option<&PropValue> {
        self.props.get(prop?)?.as_ref()
    }
}

/// A temp entity's props (only the ones it sent).
impl Props for [(usize, PropValue)] {
    fn value(&self, prop: Option<usize>) -> Option<&PropValue> {
        let prop = prop?;
        self.iter().find(|(i, _)| *i == prop).map(|(_, v)| v)
    }
}

/// What a game makes of a demo: its value for each entity it cares about
/// at each frame, and its one-shot events.
pub trait Mapper {
    /// One entity at a frame, as the game sees it.
    type Entity: Clone;
    /// One thing that happened (a shot, a sound, a message, a game event).
    type Event: Clone;

    /// The send tables arrived: look props up per class
    /// (`DemoState::flat`, `DemoState::classes`).
    fn data_tables(&mut self, state: &DemoState);

    /// Entity `index`'s value now; None leaves it out of the frame.
    fn entity(&mut self, state: &DemoState, index: u32, entity: &Entity) -> Option<Self::Entity>;

    /// The events a record says, pushed to `out` (called for every record
    /// but data tables, before a packet's entities are snapshot: an
    /// entity deleted by the packet is no longer in `state`).
    fn record(&mut self, state: &DemoState, record: &Record, out: &mut Vec<Self::Event>);
}

/// The state at one recorded tick.
#[derive(Clone, Debug)]
pub struct Frame<E, V> {
    /// Demo tick (0 at the start of recording).
    pub tick: i32,
    /// The server's tick then (`net_Tick`): server times in props
    /// (`m_flC4Blow`, ...) are this times the tick interval.
    pub server_tick: u32,
    /// Every mapped entity, by index (ascending).
    pub entities: Vec<(u32, E)>,
    /// What happened since the frame before.
    pub events: Vec<V>,
}

impl<E, V> Frame<E, V> {
    pub fn entity(&self, index: u32) -> Option<&E> {
        self.entities
            .binary_search_by_key(&index, |(i, _)| *i)
            .ok()
            .map(|i| &self.entities[i].1)
    }
}

/// A demo read into frames (see the module docs).
#[derive(Clone, Debug)]
pub struct Timeline<E, V> {
    pub header: Header,
    /// Seconds per tick (`svc_ServerInfo`).
    pub tick_interval: f32,
    /// The map the demo was recorded on (the header's).
    pub map: String,
    /// In tick order.
    pub frames: Vec<Frame<E, V>>,
    /// What failed to parse (the rest of each such packet was skipped).
    pub errors: Vec<String>,
}

/// An entity between two frames: the older value, the newer one (the same
/// when there is none) and how far between (0..1).
pub struct Between<'a, E> {
    pub from: &'a E,
    pub to: &'a E,
    pub fraction: f32,
    /// Their ticks.
    pub ticks: (i32, i32),
}

impl<E: Clone, V: Clone> Timeline<E, V> {
    /// Reads demo `data` through `mapper`. Errors inside packets are kept
    /// in `errors`; a broken file (bad header, a frame cut short) fails.
    pub fn read<M: Mapper<Entity = E, Event = V>>(data: &[u8], mapper: &mut M) -> Result<Self> {
        let mut reader = DemoReader::new(data)?;
        let header = reader.header().clone();
        let mut frames: Vec<Frame<E, V>> = Vec::new();
        let mut errors = Vec::new();
        let mut events = Vec::new();
        while let Some(item) = reader.next_item() {
            let item = item?;
            let state = reader.state();
            match &item.record {
                Record::DataTables => {
                    mapper.data_tables(state);
                    continue;
                }
                Record::Error(e) => errors.push(format!("tick {}: {e}", item.tick)),
                Record::Stop => break,
                _ => {}
            }
            events.clear();
            mapper.record(state, &item.record, &mut events);
            let snapshot = matches!(item.record, Record::Entities(_));
            if events.is_empty() && !snapshot {
                continue;
            }
            // This tick's frame: the last one, or a new one carrying the
            // state on until its own entities arrive.
            if frames.last().is_none_or(|f| f.tick != item.tick) {
                let entities = frames.last().map(|f| f.entities.clone()).unwrap_or_default();
                frames.push(Frame {
                    tick: item.tick,
                    server_tick: state.server_tick,
                    entities,
                    events: Vec::new(),
                });
            }
            let frame = frames.last_mut().expect("pushed");
            frame.events.append(&mut events);
            if snapshot {
                frame.server_tick = state.server_tick;
                frame.entities = state
                    .entities
                    .iter()
                    .enumerate()
                    .filter_map(|(i, e)| {
                        let e = e.as_ref()?;
                        Some((i as u32, mapper.entity(state, i as u32, e)?))
                    })
                    .collect();
            }
        }
        let tick_interval = reader.state().tick_interval();
        Ok(Self {
            map: header.map.clone(),
            header,
            tick_interval,
            frames,
            errors,
        })
    }

    /// Seconds from the start of recording at `tick`.
    pub fn seconds(&self, tick: f64) -> f64 {
        tick * self.tick_interval as f64
    }

    /// The first and last frames' ticks.
    pub fn ticks(&self) -> (i32, i32) {
        (
            self.frames.first().map_or(0, |f| f.tick),
            self.frames.last().map_or(0, |f| f.tick),
        )
    }

    /// The last frame at or before `tick` (the first when it is before
    /// them all); None without frames.
    pub fn frame_at(&self, tick: f64) -> Option<usize> {
        if self.frames.is_empty() {
            return None;
        }
        let after = self.frames.partition_point(|f| f.tick as f64 <= tick);
        Some(after.saturating_sub(1))
    }

    /// Entity `index` at `tick`: in the frame at or before it, eased toward
    /// the next frame that has it.
    pub fn between(&self, index: u32, tick: f64) -> Option<Between<'_, E>> {
        let i = self.frame_at(tick)?;
        let a = &self.frames[i];
        let from = a.entity(index)?;
        let next = self.frames.get(i + 1).and_then(|b| Some((b, b.entity(index)?)));
        Some(match next {
            Some((b, to)) if b.tick > a.tick => Between {
                from,
                to,
                fraction: ((tick - a.tick as f64) / (b.tick - a.tick) as f64).clamp(0.0, 1.0) as f32,
                ticks: (a.tick, b.tick),
            },
            _ => Between {
                from,
                to: from,
                fraction: 0.0,
                ticks: (a.tick, a.tick),
            },
        })
    }

    /// The server's time at demo `tick` (s), from the frame at or before it.
    pub fn server_time(&self, tick: f64) -> f64 {
        let Some(i) = self.frame_at(tick) else { return 0.0 };
        let f = &self.frames[i];
        (f.server_tick as f64 + (tick - f.tick as f64).max(0.0)) * self.tick_interval as f64
    }
}

/// The entities a packet deleted (`ChangeKind::Delete`), for a mapper's
/// `record`: what went away since the last frame.
pub fn deleted(changes: &[EntityChange]) -> impl Iterator<Item = u32> + '_ {
    changes.iter().filter(|c| c.kind == ChangeKind::Delete).map(|c| c.index)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn handles_name_an_index_or_nothing() {
        assert_eq!(handle_index(311301), Some(5));
        assert_eq!(handle_index(1583108), Some(4));
        assert_eq!(handle_index(INVALID_HANDLE), None);
        assert_eq!(handle_index(-1), None);
    }

    /// Players' origins at every frame, no events: a mapper as small as
    /// one gets.
    struct Origins {
        player: Option<u16>,
        props: ClassProps,
    }

    impl Mapper for Origins {
        type Entity = [f32; 3];
        type Event = String;

        fn data_tables(&mut self, state: &DemoState) {
            self.player = state
                .classes
                .iter()
                .position(|c| c.name == "CCSPlayer")
                .map(|i| i as u16);
            if let Some(p) = self.player {
                self.props = ClassProps::new(&state.flat[p as usize], &["csnonlocaldata.m_vecOrigin"]);
            }
        }

        fn entity(&mut self, _: &DemoState, _: u32, e: &Entity) -> Option<[f32; 3]> {
            (Some(e.class) == self.player).then(|| e.vector(self.props.at(0)))?
        }

        fn record(&mut self, _: &DemoState, record: &Record, out: &mut Vec<String>) {
            if let Record::GameEvent(e) = record {
                out.push(e.name.clone());
            }
        }
    }

    /// The sample demos (never committed; `$CHIMERA_DEMOS` (old: `$MASHUP_DEMOS`) or the
    /// workspace's `target/demos/`): frames in tick order, each player
    /// eased between frames, the server's clock running with the demo's.
    #[test]
    fn sample_demos_read_into_frames() {
        let dir = std::env::var_os("CHIMERA_DEMOS")
            .or_else(|| std::env::var_os("MASHUP_DEMOS"))
            .map(std::path::PathBuf::from)
            .unwrap_or_else(|| std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/demos"));
        let Some(data) = std::fs::read(dir.join("mashup_round_bots.dem")).ok() else {
            eprintln!("skipping: no mashup_round_bots.dem in {}", dir.display());
            return;
        };
        let mut mapper = Origins {
            player: None,
            props: ClassProps::default(),
        };
        let t = Timeline::read(&data, &mut mapper).unwrap();
        assert!(t.errors.is_empty(), "{:?}", t.errors.first());
        assert_eq!(t.map, "de_dust2");
        assert!((t.tick_interval - 0.015).abs() < 1e-6);
        assert!(t.frames.windows(2).all(|w| w[0].tick < w[1].tick));
        assert!(t.frames.len() > 2000, "{} frames", t.frames.len());
        // The bot round's game events landed in frames.
        let deaths = t
            .frames
            .iter()
            .flat_map(|f| &f.events)
            .filter(|e| *e == "player_death")
            .count();
        assert_eq!(deaths, 9);
        // Player 2 between two frames is between their origins.
        let i = t.frames.iter().position(|f| f.tick >= 1000).unwrap();
        let (a, b) = (&t.frames[i], &t.frames[i + 1]);
        let mid = (a.tick + b.tick) as f64 / 2.0;
        let between = t.between(2, mid).unwrap();
        assert_eq!(between.ticks, (a.tick, b.tick));
        assert!((between.fraction - 0.5).abs() < 1e-6);
        assert_eq!(between.from, a.entity(2).unwrap());
        assert_eq!(t.frame_at(a.tick as f64), Some(i));
        assert_eq!(t.frame_at(-5.0), Some(0));
        // The server's clock runs with the demo's.
        let s = t.server_time(b.tick as f64) - t.server_time(a.tick as f64);
        assert!((s - (b.tick - a.tick) as f64 * 0.015).abs() < 1e-6, "{s}");
    }
}
