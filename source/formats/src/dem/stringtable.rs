//! String tables: named lists of strings with optional user data, created
//! and updated by the server (`svc_CreateStringTable`,
//! `svc_UpdateStringTable`) and snapshotted whole in `dem_stringtables`.
//! The ones a demo reader uses: `soundprecache` (sound names by index),
//! `modelprecache`, `userinfo` (players) and `instancebaseline` (each
//! class's default prop values).

use super::bits::{BitReader, Result, bits_for, log2};

#[derive(Clone, Debug, Default)]
pub struct Entry {
    pub string: String,
    /// User data, packed least significant bit first.
    pub data: Vec<u8>,
    pub data_bits: usize,
}

#[derive(Clone, Debug)]
pub struct StringTable {
    pub name: String,
    pub max_entries: u32,
    /// Fixed-size user data: its width in bits.
    pub fixed_bits: Option<u32>,
    pub entries: Vec<Entry>,
}

impl StringTable {
    pub fn get(&self, index: usize) -> Option<&Entry> {
        self.entries.get(index)
    }
}

fn read_bits_packed(r: &mut BitReader, n: usize) -> Result<Vec<u8>> {
    let mut out = Vec::with_capacity(n.div_ceil(8));
    let mut left = n;
    while left > 0 {
        let take = left.min(8);
        out.push(r.bits(take as u32)? as u8);
        left -= take;
    }
    Ok(out)
}

/// Applies `count` entry changes to `table` and returns the changed
/// indices. Each entry: an index (next one, or explicit), an optional
/// string (possibly a prefix of one of the last 32 entries of this update
/// plus a suffix), optional user data.
pub fn apply_update(r: &mut BitReader, table: &mut StringTable, count: u32) -> Result<Vec<usize>> {
    let index_bits = log2(table.max_entries.max(1));
    let mut last: i64 = -1;
    let mut history: Vec<String> = Vec::new();
    let mut changed = Vec::with_capacity(count as usize);
    for _ in 0..count {
        let index = if r.bit()? {
            (last + 1) as usize
        } else {
            r.bits(index_bits)? as usize
        };
        last = index as i64;
        let string = if r.bit()? {
            if r.bit()? {
                let h = r.bits(5)? as usize;
                let keep = r.bits(5)? as usize;
                let rest = r.string()?;
                let base = history.get(h).map(|s| s.as_str()).unwrap_or("");
                let mut s: String = base.chars().take(keep).collect();
                s.push_str(&rest);
                Some(s)
            } else {
                Some(r.string()?)
            }
        } else {
            None
        };
        let data = if r.bit()? {
            Some(match table.fixed_bits {
                Some(bits) => (read_bits_packed(r, bits as usize)?, bits as usize),
                None => {
                    let len = r.bits(14)? as usize;
                    (r.bytes(len)?, len * 8)
                }
            })
        } else {
            None
        };
        if table.entries.len() <= index {
            table.entries.resize(index + 1, Entry::default());
        }
        let entry = &mut table.entries[index];
        if let Some(s) = string {
            entry.string = s;
        }
        if let Some((d, bits)) = data {
            entry.data = d;
            entry.data_bits = bits;
        }
        if history.len() == 32 {
            history.remove(0);
        }
        history.push(entry.string.clone());
        changed.push(index);
    }
    Ok(changed)
}

/// `svc_CreateStringTable` (network protocol 24): the new table and the
/// indices it filled.
pub fn read_create(r: &mut BitReader, protocol: u32) -> Result<(StringTable, Vec<usize>)> {
    let name = r.string()?;
    let max_entries = r.u16()? as u32;
    let count = r.bits(log2(max_entries.max(1)) + 1)?;
    let length = if protocol > 23 { r.var_u32()? } else { r.bits(20)? } as usize;
    let fixed_bits = if r.bit()? {
        let _size_bytes = r.bits(12)?;
        Some(r.bits(4)?)
    } else {
        None
    };
    let compressed = r.bit()?;
    let mut body = r.sub(length)?;
    let mut table = StringTable {
        name,
        max_entries,
        fixed_bits,
        entries: Vec::new(),
    };
    let changed = if compressed {
        let size = body.u32()? as usize;
        let csize = body.u32()? as usize;
        let magic = body.bytes(4)?;
        let payload = body.bytes(csize.saturating_sub(4))?;
        let plain = match &magic[..] {
            b"LZSS" => lzss(&payload).ok_or("bad LZSS data")?,
            b"SNAP" => snappy(&payload).ok_or("bad Snappy data")?,
            other => {
                return Err(format!(
                    "string table compression {:?} not supported",
                    String::from_utf8_lossy(other)
                ));
            }
        };
        if plain.len() != size {
            return Err(format!(
                "string table inflated to {} bytes, expected {size}",
                plain.len()
            ));
        }
        let mut pr = BitReader::new(&plain);
        apply_update(&mut pr, &mut table, count)?
    } else {
        apply_update(&mut body, &mut table, count)?
    };
    Ok((table, changed))
}

/// `svc_UpdateStringTable`: (table id, change count, body reader).
pub fn read_update_header<'a>(r: &mut BitReader<'a>) -> Result<(usize, u32, BitReader<'a>)> {
    let id = r.bits(5)? as usize;
    let count = if r.bit()? { r.u16()? as u32 } else { 1 };
    let length = r.bits(20)? as usize;
    Ok((id, count, r.sub(length)?))
}

/// Valve's LZSS: a u32 size, then groups of 8 items led by a flag byte
/// (bit set: a 12-bit back offset and 4-bit length; clear: a literal).
pub fn lzss(input: &[u8]) -> Option<Vec<u8>> {
    let size = u32::from_le_bytes(input.get(0..4)?.try_into().ok()?) as usize;
    let mut out = Vec::with_capacity(size);
    let mut pos = 4;
    let mut next = || {
        let b = *input.get(pos)?;
        pos += 1;
        Some(b)
    };
    'outer: while out.len() < size {
        let mut cmd = next()?;
        for _ in 0..8 {
            if cmd & 1 == 1 {
                let hi = next()? as usize;
                let lo = next()? as usize;
                let offset = (hi << 4) | (lo >> 4);
                let count = (lo & 0xF) + 1;
                if count == 1 {
                    break 'outer;
                }
                let start = out.len().checked_sub(offset + 1)?;
                for i in 0..count {
                    let b = *out.get(start + i)?;
                    out.push(b);
                }
            } else {
                out.push(next()?);
            }
            if out.len() >= size {
                break 'outer;
            }
            cmd >>= 1;
        }
    }
    Some(out)
}

/// Raw Snappy (no framing): a varint size, then literals and copies
/// (1-, 2- or 4-byte offsets) by each tag byte's low two bits.
pub fn snappy(input: &[u8]) -> Option<Vec<u8>> {
    let mut pos = 0;
    let mut size = 0usize;
    for shift in (0..35).step_by(7) {
        let b = *input.get(pos)?;
        pos += 1;
        size |= ((b & 0x7F) as usize) << shift;
        if b & 0x80 == 0 {
            break;
        }
    }
    let le = |b: &[u8]| b.iter().rev().fold(0usize, |acc, &x| (acc << 8) | x as usize);
    let mut out = Vec::with_capacity(size);
    while pos < input.len() {
        let tag = input[pos];
        pos += 1;
        let (len, offset) = match tag & 3 {
            0 => {
                let mut len = (tag >> 2) as usize;
                if len >= 60 {
                    let n = len - 59;
                    len = le(input.get(pos..pos + n)?);
                    pos += n;
                }
                let len = len + 1;
                out.extend_from_slice(input.get(pos..pos + len)?);
                pos += len;
                continue;
            }
            1 => {
                let len = 4 + ((tag >> 2) & 7) as usize;
                let offset = (((tag >> 5) as usize) << 8) | *input.get(pos)? as usize;
                pos += 1;
                (len, offset)
            }
            2 => {
                let offset = le(input.get(pos..pos + 2)?);
                pos += 2;
                (1 + (tag >> 2) as usize, offset)
            }
            _ => {
                let offset = le(input.get(pos..pos + 4)?);
                pos += 4;
                (1 + (tag >> 2) as usize, offset)
            }
        };
        let start = out.len().checked_sub(offset).filter(|_| offset > 0)?;
        for i in 0..len {
            let b = out[start + i];
            out.push(b);
        }
    }
    (out.len() == size).then_some(out)
}

/// The `dem_stringtables` frame: every table with all its entries (and
/// client-side entries, skipped). Returns (name, entries).
pub fn read_snapshot(r: &mut BitReader) -> Result<Vec<(String, Vec<Entry>)>> {
    let count = r.u8()?;
    let mut tables = Vec::new();
    for _ in 0..count {
        let name = r.string()?;
        let n = r.u16()?;
        let mut entries = Vec::with_capacity(n as usize);
        for _ in 0..n {
            let string = r.string()?;
            let (data, data_bits) = if r.bit()? {
                let len = r.u16()? as usize;
                (r.bytes(len)?, len * 8)
            } else {
                (Vec::new(), 0)
            };
            entries.push(Entry {
                string,
                data,
                data_bits,
            });
        }
        if r.bit()? {
            let n = r.u16()?;
            for _ in 0..n {
                r.string()?;
                if r.bit()? {
                    let len = r.u16()? as usize;
                    r.skip(len * 8)?;
                }
            }
        }
        tables.push((name, entries));
    }
    Ok(tables)
}

/// A `userinfo` entry's player: (name, user id, Steam id text, fake player).
pub fn parse_userinfo(data: &[u8]) -> Option<(String, i32, String, bool)> {
    // player_info_t: name[32], userID i32, guid[33], friendsID u32,
    // friendsName[32], fakeplayer bool, ishltv bool, ...
    if data.len() < 32 + 4 + 33 + 3 + 4 + 32 + 2 {
        return None;
    }
    let cstr = |b: &[u8]| {
        let end = b.iter().position(|&c| c == 0).unwrap_or(b.len());
        String::from_utf8_lossy(&b[..end]).into_owned()
    };
    let name = cstr(&data[0..32]);
    let user_id = i32::from_le_bytes(data[32..36].try_into().ok()?);
    let guid = cstr(&data[36..69]);
    // guid[33] then 3 bytes of padding to align friendsID.
    let fake = data[72 + 4 + 32] != 0;
    Some((name, user_id, guid, fake))
}

/// Bits of a class id in `instancebaseline` and entity headers.
pub fn class_bits(classes: u32) -> u32 {
    bits_for(classes)
}

#[cfg(test)]
mod tests {
    use super::super::bits::tests::pack;
    use super::*;

    fn s(text: &str) -> Vec<(u32, u32)> {
        text.bytes().chain([0]).map(|b| (b as u32, 8)).collect()
    }

    #[test]
    fn update_with_history_prefix() {
        let mut table = StringTable {
            name: "t".into(),
            max_entries: 16,
            fixed_bits: None,
            entries: Vec::new(),
        };
        let mut f = vec![(1, 1), (1, 1), (0, 1)];
        f.extend(s("weapons/ak47"));
        f.push((0, 1)); // no data
        // entry 1: next index, string from history 0 keeping 8 chars + "m4a1"
        f.extend([(1, 1), (1, 1), (1, 1), (0, 5), (8, 5)]);
        f.extend(s("m4a1"));
        f.extend([(1, 1), (2, 14), (0xAB, 8), (0xCD, 8)]);
        let data = pack(&f);
        let changed = apply_update(&mut BitReader::new(&data), &mut table, 2).unwrap();
        assert_eq!(changed, vec![0, 1]);
        assert_eq!(table.entries[0].string, "weapons/ak47");
        assert_eq!(table.entries[1].string, "weapons/m4a1");
        assert_eq!(table.entries[1].data, vec![0xAB, 0xCD]);
    }

    #[test]
    fn snappy_literal_and_copy() {
        // size 9: literal "abc" (tag (3-1)<<2), copy1 len 6 offset 3.
        let input = [9, 2 << 2, b'a', b'b', b'c', 0b001 | ((6 - 4) << 2), 3];
        assert_eq!(snappy(&input).unwrap(), b"abcabcabc");
    }

    #[test]
    fn lzss_literals_and_backref() {
        // size 6: literals "abc", then back-ref offset 2 (to 'a'), count 3.
        let mut input = 6u32.to_le_bytes().to_vec();
        input.extend([0b1000, b'a', b'b', b'c', 0x00, 0x22]);
        assert_eq!(lzss(&input).unwrap(), b"abcabc");
    }
}
