//! Entity state from `svc_PacketEntities` (and the one-shot
//! `svc_TempEntities`): each entity's class and current prop values,
//! rebuilt from baselines and deltas the way a client does.
//!
//! An entering entity starts from the instance baseline the packet names
//! (two are kept; a packet may ask for its entering entities to become the
//! other one) when that holds the same class, else from its class's
//! static baseline (the `instancebaseline` string table), then applies
//! the props sent. A non-delta packet (full update) replaces every entity.

use super::bits::{BitReader, Result};
use super::sendtable::{PropValue, read_props};
use super::{DemoState, Record};
use std::collections::HashMap;

pub const MAX_EDICTS: usize = 2048;

#[derive(Clone, Debug)]
pub struct Entity {
    pub class: u16,
    pub serial: u32,
    pub in_pvs: bool,
    /// Current value per flattened prop index of the class (None: never sent).
    pub props: Vec<Option<PropValue>>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ChangeKind {
    Enter,
    Update,
    Leave,
    Delete,
}

/// One entity touched by a packet: what happened and which prop indices
/// were sent (sent is not always changed: compare values to tell).
#[derive(Clone, Debug)]
pub struct EntityChange {
    pub index: u32,
    pub kind: ChangeKind,
    pub props: Vec<usize>,
}

/// A temp entity (an effect sent once, e.g. `CTEEffectDispatch`).
#[derive(Clone, Debug)]
pub struct TempEntity {
    pub class: u16,
    pub delay: f32,
    pub props: Vec<(usize, PropValue)>,
}

#[derive(Default)]
pub struct Baselines {
    /// Instance baselines (two, by the packet's baseline bit): entity
    /// index to (class, values).
    pub instance: [HashMap<u32, (u16, Vec<Option<PropValue>>)>; 2],
    /// Decoded static baselines by class id.
    pub statics: HashMap<u16, Vec<Option<PropValue>>>,
}

fn static_baseline(st: &mut DemoState, class: u16) -> Result<Vec<Option<PropValue>>> {
    if let Some(b) = st.baselines.statics.get(&class) {
        return Ok(b.clone());
    }
    let flat = st
        .flat
        .get(class as usize)
        .ok_or_else(|| format!("unknown class {class}"))?;
    let mut values = vec![None; flat.len()];
    let entry = st
        .table("instancebaseline")
        .and_then(|t| t.entries.iter().find(|e| e.string.parse::<u16>().ok() == Some(class)));
    if let Some(e) = entry {
        let mut r = BitReader::new(&e.data);
        let mut props = Vec::new();
        read_props(&mut r, flat, &mut props).map_err(|err| format!("static baseline of class {class}: {err}"))?;
        for (i, v) in props {
            values[i] = Some(v);
        }
    }
    st.baselines.statics.insert(class, values.clone());
    Ok(values)
}

pub(super) fn read_packet_entities(st: &mut DemoState, r: &mut BitReader) -> Result<Record> {
    let _max_entries = r.bits(11)?;
    let is_delta = r.bit()?;
    if is_delta {
        let _delta_from = r.i32()?;
    }
    let baseline = r.bit()? as usize;
    let updated = r.bits(11)?;
    let length = r.bits(20)? as usize;
    let update_baseline = r.bit()?;
    let mut d = r.sub(length)?;

    if !is_delta {
        st.entities.iter_mut().for_each(|e| *e = None);
    }
    if update_baseline {
        st.baselines.instance[1 - baseline] = st.baselines.instance[baseline].clone();
    }

    let mut changes = Vec::with_capacity(updated as usize);
    let mut index: i64 = -1;
    let mut props = Vec::new();
    for _ in 0..updated {
        index += d.ubitvar()? as i64 + 1;
        if index as usize >= MAX_EDICTS {
            return Err(format!("entity index {index} out of range"));
        }
        let i = index as usize;
        let leave = d.bit()?;
        let other = d.bit()?;
        props.clear();
        match (leave, other) {
            (false, true) => {
                let class = d.bits(st.class_bits)? as u16;
                let serial = d.bits(10)?;
                let mut values = match st.baselines.instance[baseline].get(&(i as u32)) {
                    Some((c, v)) if *c == class => v.clone(),
                    _ => static_baseline(st, class)?,
                };
                let flat = st
                    .flat
                    .get(class as usize)
                    .ok_or_else(|| format!("unknown class {class}"))?;
                values.resize(flat.len(), None);
                read_props(&mut d, flat, &mut props)?;
                let mut sent = Vec::with_capacity(props.len());
                for (pi, v) in props.drain(..) {
                    values[pi] = Some(v);
                    sent.push(pi);
                }
                if update_baseline {
                    st.baselines.instance[1 - baseline].insert(i as u32, (class, values.clone()));
                }
                // SourceTV re-sends live entities as entering (baseline plus
                // everything that differs from it): same class and serial is
                // an update of what changed, not a new entity.
                let change = match &st.entities[i] {
                    Some(old) if old.class == class && old.serial == serial => {
                        let changed = (0..values.len())
                            .filter(|&k| old.props.get(k) != Some(&values[k]))
                            .collect();
                        EntityChange {
                            index: i as u32,
                            kind: ChangeKind::Update,
                            props: changed,
                        }
                    }
                    _ => EntityChange {
                        index: i as u32,
                        kind: ChangeKind::Enter,
                        props: sent,
                    },
                };
                changes.push(change);
                st.entities[i] = Some(Entity {
                    class,
                    serial,
                    in_pvs: true,
                    props: values,
                });
            }
            (false, false) => {
                let e = st.entities[i]
                    .as_mut()
                    .ok_or_else(|| format!("delta for absent entity {i}"))?;
                let flat = st
                    .flat
                    .get(e.class as usize)
                    .ok_or_else(|| format!("unknown class {}", e.class))?;
                read_props(&mut d, flat, &mut props)?;
                let mut sent = Vec::with_capacity(props.len());
                for (pi, v) in props.drain(..) {
                    e.props[pi] = Some(v);
                    sent.push(pi);
                }
                e.in_pvs = true;
                changes.push(EntityChange {
                    index: i as u32,
                    kind: ChangeKind::Update,
                    props: sent,
                });
            }
            (true, false) => {
                if let Some(e) = st.entities[i].as_mut() {
                    e.in_pvs = false;
                }
                changes.push(EntityChange {
                    index: i as u32,
                    kind: ChangeKind::Leave,
                    props: Vec::new(),
                });
            }
            (true, true) => {
                st.entities[i] = None;
                changes.push(EntityChange {
                    index: i as u32,
                    kind: ChangeKind::Delete,
                    props: Vec::new(),
                });
            }
        }
    }
    if is_delta {
        while d.bit()? {
            let i = d.bits(11)? as usize;
            st.entities[i] = None;
            changes.push(EntityChange {
                index: i as u32,
                kind: ChangeKind::Delete,
                props: Vec::new(),
            });
        }
    }
    if d.bits_left() != 0 {
        return Err(format!("{} bits left after {updated} entity updates", d.bits_left()));
    }
    Ok(Record::Entities(changes))
}

pub(super) fn read_temp_entities(st: &mut DemoState, r: &mut BitReader) -> Result<Record> {
    let count = r.u8()?;
    let length = if st.protocol > 23 { r.var_u32()? } else { r.bits(17)? } as usize;
    let mut d = r.sub(length)?;
    let count = if count == 0 { 1 } else { count };
    let mut out: Vec<TempEntity> = Vec::with_capacity(count as usize);
    for _ in 0..count {
        let delay = if d.bit()? { d.u8()? as f32 / 100.0 } else { 0.0 };
        let class = if d.bit()? {
            (d.bits(st.class_bits)? as u16).wrapping_sub(1)
        } else {
            out.last().map(|t| t.class).ok_or("temp entity without a class")?
        };
        let flat = st
            .flat
            .get(class as usize)
            .ok_or_else(|| format!("unknown temp entity class {class}"))?;
        let mut props = Vec::new();
        read_props(&mut d, flat, &mut props)?;
        out.push(TempEntity { class, delay, props });
    }
    Ok(Record::TempEntities(out))
}
