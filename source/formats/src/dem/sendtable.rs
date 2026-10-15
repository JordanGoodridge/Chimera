//! Send tables (the `dem_datatables` frame): each server class's network
//! fields, flattened into the ordered prop list that entity updates index
//! into, and the decoding of one prop value.
//!
//! Flattening follows the public description of the Source network format:
//! drop excluded props, walk data tables depth first (a collapsible one's
//! props join their parent's run, others are appended after it), then move
//! the `CHANGES_OFTEN` props to the front by swapping.

use super::bits::{BitReader, Result, bits_for};
use std::fmt;

pub const SPROP_UNSIGNED: u32 = 1 << 0;
pub const SPROP_COORD: u32 = 1 << 1;
pub const SPROP_NOSCALE: u32 = 1 << 2;
pub const SPROP_NORMAL: u32 = 1 << 5;
pub const SPROP_EXCLUDE: u32 = 1 << 6;
pub const SPROP_INSIDEARRAY: u32 = 1 << 8;
pub const SPROP_CHANGES_OFTEN: u32 = 1 << 10;
pub const SPROP_COLLAPSIBLE: u32 = 1 << 12;
pub const SPROP_COORD_MP: u32 = 1 << 13;
pub const SPROP_COORD_MP_LOWPRECISION: u32 = 1 << 14;
pub const SPROP_COORD_MP_INTEGRAL: u32 = 1 << 15;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PropKind {
    Int,
    Float,
    Vector,
    VectorXY,
    String,
    Array,
    DataTable,
}

impl PropKind {
    fn from_raw(v: u32) -> Result<Self> {
        Ok(match v {
            0 => PropKind::Int,
            1 => PropKind::Float,
            2 => PropKind::Vector,
            3 => PropKind::VectorXY,
            4 => PropKind::String,
            5 => PropKind::Array,
            6 => PropKind::DataTable,
            _ => return Err(format!("unknown send prop type {v}")),
        })
    }
}

/// One prop as a send table declares it.
#[derive(Clone, Debug)]
pub struct SendProp {
    pub kind: PropKind,
    pub name: String,
    pub flags: u32,
    /// Data tables: the table it holds; excludes: the table of the excluded prop.
    pub table: Option<String>,
    pub low: f32,
    pub high: f32,
    pub bits: u32,
    pub elements: u32,
    /// Arrays: the element prop (declared just before the array).
    pub element: Option<Box<SendProp>>,
}

#[derive(Clone, Debug)]
pub struct SendTable {
    pub name: String,
    pub needs_decoder: bool,
    pub props: Vec<SendProp>,
}

#[derive(Clone, Debug)]
pub struct ServerClass {
    pub id: u16,
    pub name: String,
    pub table: String,
}

/// A prop of a class's flattened list: its path (data table props passed
/// through, `baseclass` left out, joined with `.`), the table that declared
/// it, and how to decode it.
#[derive(Clone, Debug)]
pub struct FlatProp {
    pub path: String,
    pub owner: String,
    pub prop: SendProp,
}

impl FlatProp {
    /// The prop's own name (the last part of its path).
    pub fn name(&self) -> &str {
        &self.prop.name
    }
}

/// A decoded prop value.
#[derive(Clone, Debug, PartialEq)]
pub enum PropValue {
    Int(i64),
    Float(f32),
    Vector([f32; 3]),
    VectorXY([f32; 2]),
    String(String),
    Array(Vec<PropValue>),
}

impl fmt::Display for PropValue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            PropValue::Int(v) => write!(f, "{v}"),
            PropValue::Float(v) => write!(f, "{v}"),
            PropValue::Vector([x, y, z]) => write!(f, "({x}, {y}, {z})"),
            PropValue::VectorXY([x, y]) => write!(f, "({x}, {y})"),
            PropValue::String(s) => write!(f, "{s:?}"),
            PropValue::Array(items) => {
                write!(f, "[")?;
                for (i, v) in items.iter().enumerate() {
                    if i > 0 {
                        write!(f, ", ")?;
                    }
                    write!(f, "{v}")?;
                }
                write!(f, "]")
            }
        }
    }
}

fn read_prop(r: &mut BitReader) -> Result<SendProp> {
    let kind = PropKind::from_raw(r.bits(5)?)?;
    let name = r.string()?;
    let flags = r.bits(16)?;
    let mut prop = SendProp {
        kind,
        name,
        flags,
        table: None,
        low: 0.0,
        high: 0.0,
        bits: 0,
        elements: 0,
        element: None,
    };
    if kind == PropKind::DataTable || flags & SPROP_EXCLUDE != 0 {
        prop.table = Some(r.string()?);
    } else if kind == PropKind::Array {
        prop.elements = r.bits(10)?;
    } else {
        prop.low = r.f32()?;
        prop.high = r.f32()?;
        prop.bits = r.bits(7)?;
    }
    Ok(prop)
}

fn read_table(r: &mut BitReader) -> Result<SendTable> {
    let needs_decoder = r.bit()?;
    let name = r.string()?;
    let count = r.bits(10)?;
    let mut props = Vec::new();
    let mut element: Option<SendProp> = None;
    for _ in 0..count {
        let mut prop = read_prop(r)?;
        if prop.flags & SPROP_INSIDEARRAY != 0 {
            element = Some(prop);
            continue;
        }
        if prop.kind == PropKind::Array {
            prop.element = element.take().map(Box::new);
        }
        props.push(prop);
    }
    Ok(SendTable {
        name,
        needs_decoder,
        props,
    })
}

/// The `dem_datatables` payload: send tables, then server classes.
pub fn read_data_tables(r: &mut BitReader) -> Result<(Vec<SendTable>, Vec<ServerClass>)> {
    let mut tables = Vec::new();
    while r.bit()? {
        tables.push(read_table(r)?);
    }
    let count = r.u16()?;
    let mut classes = Vec::with_capacity(count as usize);
    for _ in 0..count {
        let id = r.u16()?;
        let name = r.string()?;
        let table = r.string()?;
        classes.push(ServerClass { id, name, table });
    }
    Ok((tables, classes))
}

/// Flattens `root` (by name) against `tables`.
pub fn flatten(tables: &[SendTable], root: &str) -> Result<Vec<FlatProp>> {
    let find = |name: &str| tables.iter().find(|t| t.name == name);
    let root_table = find(root).ok_or_else(|| format!("no send table {root}"))?;

    let mut excludes = Vec::new();
    fn gather<'a>(
        t: &'a SendTable,
        find: &dyn Fn(&str) -> Option<&'a SendTable>,
        seen: &mut Vec<&'a str>,
        out: &mut Vec<(String, String)>,
    ) {
        seen.push(&t.name);
        for p in &t.props {
            if p.flags & SPROP_EXCLUDE != 0 {
                out.push((p.table.clone().unwrap_or_default(), p.name.clone()));
            } else if p.kind == PropKind::DataTable
                && let Some(sub) = p.table.as_deref().and_then(find)
                && !seen.contains(&sub.name.as_str())
            {
                gather(sub, find, seen, out);
            }
        }
    }
    gather(root_table, &find, &mut Vec::new(), &mut excludes);

    struct Ctx<'a, 'b> {
        find: &'b dyn Fn(&str) -> Option<&'a SendTable>,
        excludes: &'b [(String, String)],
    }
    fn end<'a>(t: &'a SendTable, ctx: &Ctx<'a, '_>, prefix: &str, out: &mut Vec<FlatProp>, stack: &mut Vec<&'a str>) {
        let mut local = Vec::new();
        collapse(t, ctx, prefix, &mut local, out, stack);
        out.extend(local);
    }
    fn collapse<'a>(
        t: &'a SendTable,
        ctx: &Ctx<'a, '_>,
        prefix: &str,
        local: &mut Vec<FlatProp>,
        out: &mut Vec<FlatProp>,
        stack: &mut Vec<&'a str>,
    ) {
        stack.push(&t.name);
        for p in &t.props {
            if p.flags & SPROP_EXCLUDE != 0 {
                continue;
            }
            if ctx.excludes.iter().any(|(tn, pn)| *tn == t.name && *pn == p.name) {
                continue;
            }
            if p.kind == PropKind::DataTable {
                let Some(sub) = p.table.as_deref().and_then(ctx.find) else {
                    continue;
                };
                if stack.contains(&sub.name.as_str()) {
                    continue;
                }
                let sub_prefix = if p.name == "baseclass" {
                    prefix.to_string()
                } else {
                    format!("{prefix}{}.", p.name)
                };
                if p.flags & SPROP_COLLAPSIBLE != 0 {
                    collapse(sub, ctx, &sub_prefix, local, out, stack);
                } else {
                    end(sub, ctx, &sub_prefix, out, stack);
                }
            } else {
                local.push(FlatProp {
                    path: format!("{prefix}{}", p.name),
                    owner: t.name.clone(),
                    prop: p.clone(),
                });
            }
        }
        stack.pop();
    }

    let ctx = Ctx {
        find: &find,
        excludes: &excludes,
    };
    let mut flat = Vec::new();
    end(root_table, &ctx, "", &mut flat, &mut Vec::new());

    let mut start = 0;
    for i in 0..flat.len() {
        if flat[i].prop.flags & SPROP_CHANGES_OFTEN != 0 {
            flat.swap(i, start);
            start += 1;
        }
    }
    Ok(flat)
}

fn read_float(r: &mut BitReader, p: &SendProp) -> Result<f32> {
    let f = p.flags;
    if f & SPROP_COORD != 0 {
        r.bit_coord()
    } else if f & SPROP_COORD_MP != 0 {
        r.bit_coord_mp(false, false)
    } else if f & SPROP_COORD_MP_LOWPRECISION != 0 {
        r.bit_coord_mp(false, true)
    } else if f & SPROP_COORD_MP_INTEGRAL != 0 {
        r.bit_coord_mp(true, false)
    } else if f & SPROP_NOSCALE != 0 {
        r.f32()
    } else if f & SPROP_NORMAL != 0 {
        r.bit_normal()
    } else {
        let raw = r.bits(p.bits)?;
        let max = ((1u64 << p.bits) - 1) as f32;
        Ok(p.low + (p.high - p.low) * (raw as f32 / max))
    }
}

/// Reads one value of `p`.
pub fn read_value(r: &mut BitReader, p: &SendProp) -> Result<PropValue> {
    Ok(match p.kind {
        PropKind::Int => {
            if p.flags & SPROP_NORMAL != 0 {
                // SPROP_VARINT shares the bit with SPROP_NORMAL on ints.
                let v = r.var_u32()?;
                if p.flags & SPROP_UNSIGNED != 0 {
                    PropValue::Int(v as i64)
                } else {
                    PropValue::Int(((v >> 1) as i32 ^ -((v & 1) as i32)) as i64)
                }
            } else if p.flags & SPROP_UNSIGNED != 0 {
                PropValue::Int(r.bits(p.bits)? as i64)
            } else {
                PropValue::Int(r.signed(p.bits)? as i64)
            }
        }
        PropKind::Float => PropValue::Float(read_float(r, p)?),
        PropKind::Vector => {
            let x = read_float(r, p)?;
            let y = read_float(r, p)?;
            let z = if p.flags & SPROP_NORMAL != 0 {
                let negative = r.bit()?;
                let sq = x * x + y * y;
                let z = if sq < 1.0 { (1.0 - sq).sqrt() } else { 0.0 };
                if negative { -z } else { z }
            } else {
                read_float(r, p)?
            };
            PropValue::Vector([x, y, z])
        }
        PropKind::VectorXY => PropValue::VectorXY([read_float(r, p)?, read_float(r, p)?]),
        PropKind::String => {
            let len = r.bits(9)? as usize;
            let b = r.bytes(len)?;
            PropValue::String(String::from_utf8_lossy(&b).into_owned())
        }
        PropKind::Array => {
            let element = p
                .element
                .as_deref()
                .ok_or_else(|| format!("array {} without element prop", p.name))?;
            let count = r.bits(bits_for(p.elements))?;
            let mut items = Vec::with_capacity(count as usize);
            for _ in 0..count {
                items.push(read_value(r, element)?);
            }
            PropValue::Array(items)
        }
        PropKind::DataTable => return Err(format!("data table prop {} in a flat list", p.name)),
    })
}

/// Reads a prop update list (entity enter/delta, baselines, temp
/// entities): while a bit is set, an index step then the value.
pub fn read_props(r: &mut BitReader, flat: &[FlatProp], out: &mut Vec<(usize, PropValue)>) -> Result<()> {
    let mut index: i64 = -1;
    while r.bit()? {
        index += r.ubitvar()? as i64 + 1;
        let fp = flat
            .get(index as usize)
            .ok_or_else(|| format!("prop index {index} past the {} props", flat.len()))?;
        out.push((index as usize, read_value(r, &fp.prop)?));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::super::bits::tests::pack;
    use super::*;

    fn prop(kind: PropKind, name: &str, flags: u32, bits: u32) -> SendProp {
        SendProp {
            kind,
            name: name.into(),
            flags,
            table: None,
            low: 0.0,
            high: 0.0,
            bits,
            elements: 0,
            element: None,
        }
    }

    fn table(name: &str, props: Vec<SendProp>) -> SendTable {
        SendTable {
            name: name.into(),
            needs_decoder: false,
            props,
        }
    }

    fn dt(name: &str, table: &str, flags: u32) -> SendProp {
        SendProp {
            table: Some(table.into()),
            ..prop(PropKind::DataTable, name, flags, 0)
        }
    }

    #[test]
    fn flattening_orders_excludes_and_changes_often() {
        let tables = vec![
            table(
                "DT_Base",
                vec![
                    prop(PropKind::Int, "m_a", 0, 8),
                    prop(PropKind::Int, "m_b", 0, 8),
                    prop(PropKind::Int, "m_fast", SPROP_CHANGES_OFTEN, 8),
                ],
            ),
            table("DT_Local", vec![prop(PropKind::Float, "m_local", SPROP_NOSCALE, 32)]),
            table(
                "DT_Player",
                vec![
                    dt("baseclass", "DT_Base", SPROP_COLLAPSIBLE),
                    SendProp {
                        table: Some("DT_Base".into()),
                        ..prop(PropKind::Int, "m_b", SPROP_EXCLUDE, 0)
                    },
                    dt("m_Local", "DT_Local", 0),
                    prop(PropKind::Int, "m_c", 0, 4),
                ],
            ),
        ];
        let flat = flatten(&tables, "DT_Player").unwrap();
        let names: Vec<_> = flat.iter().map(|f| f.path.as_str()).collect();
        // m_Local's run is appended before the collapsed run (base + own).
        assert_eq!(names, ["m_fast", "m_a", "m_Local.m_local", "m_c"]);
    }

    #[test]
    fn prop_update_list() {
        let flat = vec![
            FlatProp {
                path: "m_a".into(),
                owner: "T".into(),
                prop: prop(PropKind::Int, "m_a", SPROP_UNSIGNED, 8),
            },
            FlatProp {
                path: "m_b".into(),
                owner: "T".into(),
                prop: prop(PropKind::Int, "m_b", 0, 4),
            },
            FlatProp {
                path: "m_c".into(),
                owner: "T".into(),
                prop: prop(PropKind::Int, "m_c", SPROP_UNSIGNED, 3),
            },
        ];
        // set m_a = 200 (step 0), skip m_b, m_c = 5 (step 1), end
        let data = pack(&[(1, 1), (0, 2), (0, 4), (200, 8), (1, 1), (0, 2), (1, 4), (5, 3), (0, 1)]);
        let mut out = Vec::new();
        read_props(&mut BitReader::new(&data), &flat, &mut out).unwrap();
        assert_eq!(out, vec![(0, PropValue::Int(200)), (2, PropValue::Int(5))]);
    }

    #[test]
    fn scaled_float() {
        let p = SendProp {
            low: 0.0,
            high: 100.0,
            ..prop(PropKind::Float, "f", 0, 8)
        };
        let data = pack(&[(255, 8)]);
        assert_eq!(
            read_value(&mut BitReader::new(&data), &p).unwrap(),
            PropValue::Float(100.0)
        );
    }
}
