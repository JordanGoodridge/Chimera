//! The little-endian bit reader every part of a demo is read with: bits
//! fill each byte from its lowest bit, multi-bit values are least
//! significant bit first, strings are byte-wide and not aligned.

pub type Result<T> = std::result::Result<T, String>;

/// A window of bits over a byte slice. `sub` hands out a reader over the
/// next `n` bits (a message body) and skips them here.
#[derive(Clone)]
pub struct BitReader<'a> {
    data: &'a [u8],
    pos: usize,
    end: usize,
}

impl<'a> BitReader<'a> {
    pub fn new(data: &'a [u8]) -> Self {
        BitReader {
            data,
            pos: 0,
            end: data.len() * 8,
        }
    }

    /// Position in bits from the start of the slice.
    pub fn pos(&self) -> usize {
        self.pos
    }

    pub fn bits_left(&self) -> usize {
        self.end - self.pos
    }

    fn need(&self, n: usize) -> Result<()> {
        if n > self.bits_left() {
            Err(format!("read past the end: {n} bits wanted, {} left", self.bits_left()))
        } else {
            Ok(())
        }
    }

    pub fn skip(&mut self, n: usize) -> Result<()> {
        self.need(n)?;
        self.pos += n;
        Ok(())
    }

    /// A reader over the next `n` bits; this one moves past them.
    pub fn sub(&mut self, n: usize) -> Result<BitReader<'a>> {
        self.need(n)?;
        let r = BitReader {
            data: self.data,
            pos: self.pos,
            end: self.pos + n,
        };
        self.pos += n;
        Ok(r)
    }

    /// Up to 32 bits as an unsigned value.
    pub fn bits(&mut self, n: u32) -> Result<u32> {
        debug_assert!(n <= 32);
        let n = n as usize;
        self.need(n)?;
        let mut value: u64 = 0;
        let mut got = 0;
        while got < n {
            let byte = self.data[self.pos / 8] as u64;
            let off = self.pos % 8;
            let take = (8 - off).min(n - got);
            value |= ((byte >> off) & ((1 << take) - 1)) << got;
            got += take;
            self.pos += take;
        }
        Ok(value as u32)
    }

    /// `n` bits, sign-extended.
    pub fn signed(&mut self, n: u32) -> Result<i32> {
        let v = self.bits(n)?;
        if n == 0 || n >= 32 {
            return Ok(v as i32);
        }
        let shift = 32 - n;
        Ok(((v << shift) as i32) >> shift)
    }

    pub fn bit(&mut self) -> Result<bool> {
        Ok(self.bits(1)? == 1)
    }

    pub fn u8(&mut self) -> Result<u8> {
        Ok(self.bits(8)? as u8)
    }

    pub fn u16(&mut self) -> Result<u16> {
        Ok(self.bits(16)? as u16)
    }

    pub fn i16(&mut self) -> Result<i16> {
        Ok(self.bits(16)? as u16 as i16)
    }

    pub fn u32(&mut self) -> Result<u32> {
        self.bits(32)
    }

    pub fn i32(&mut self) -> Result<i32> {
        Ok(self.bits(32)? as i32)
    }

    pub fn f32(&mut self) -> Result<f32> {
        Ok(f32::from_bits(self.bits(32)?))
    }

    pub fn bytes(&mut self, n: usize) -> Result<Vec<u8>> {
        self.need(n * 8)?;
        if self.pos.is_multiple_of(8) {
            let start = self.pos / 8;
            self.pos += n * 8;
            return Ok(self.data[start..start + n].to_vec());
        }
        (0..n).map(|_| self.u8()).collect()
    }

    /// A NUL-terminated string (lossy UTF-8). Ends without a NUL at the end
    /// of the window.
    pub fn string(&mut self) -> Result<String> {
        let mut out = Vec::new();
        while self.bits_left() >= 8 {
            let b = self.u8()?;
            if b == 0 {
                break;
            }
            out.push(b);
        }
        Ok(String::from_utf8_lossy(&out).into_owned())
    }

    /// A string in a fixed field of `n` bytes, cut at the first NUL.
    pub fn fixed_string(&mut self, n: usize) -> Result<String> {
        let b = self.bytes(n)?;
        let end = b.iter().position(|&c| c == 0).unwrap_or(b.len());
        Ok(String::from_utf8_lossy(&b[..end]).into_owned())
    }

    /// Protobuf-style variable-length integer: 7 bits per byte, high bit
    /// set while more follow (at most 5 bytes).
    pub fn var_u32(&mut self) -> Result<u32> {
        let mut result = 0u32;
        for i in 0..5 {
            let b = self.u8()?;
            result |= ((b & 0x7F) as u32) << (7 * i);
            if b & 0x80 == 0 {
                break;
            }
        }
        Ok(result)
    }

    /// The entity and prop index step: a 2-bit width selector (4, 8, 12 or
    /// 32 bits) then the value.
    pub fn ubitvar(&mut self) -> Result<u32> {
        let width = match self.bits(2)? {
            0 => 4,
            1 => 8,
            2 => 12,
            _ => 32,
        };
        self.bits(width)
    }

    /// A world coordinate (`SPROP_COORD`): integer and fraction flags, sign,
    /// 14-bit integer part (stored minus one), 5-bit fraction.
    pub fn bit_coord(&mut self) -> Result<f32> {
        let has_int = self.bit()?;
        let has_frac = self.bit()?;
        if !has_int && !has_frac {
            return Ok(0.0);
        }
        let negative = self.bit()?;
        let int = if has_int { self.bits(14)? + 1 } else { 0 };
        let frac = if has_frac { self.bits(5)? } else { 0 };
        let v = int as f32 + frac as f32 / 32.0;
        Ok(if negative { -v } else { v })
    }

    /// `SPROP_COORD_MP` and its low-precision and integral variants.
    pub fn bit_coord_mp(&mut self, integral: bool, low_precision: bool) -> Result<f32> {
        let in_bounds = self.bit()?;
        let has_int = self.bit()?;
        let int_bits = if in_bounds { 11 } else { 14 };
        let mut negative = false;
        let mut value = 0.0;
        if integral {
            if has_int {
                negative = self.bit()?;
                value = (self.bits(int_bits)? + 1) as f32;
            }
        } else {
            negative = self.bit()?;
            if has_int {
                value = (self.bits(int_bits)? + 1) as f32;
            }
            let frac_bits = if low_precision { 3 } else { 5 };
            value += self.bits(frac_bits)? as f32 / (1u32 << frac_bits) as f32;
        }
        Ok(if negative { -value } else { value })
    }

    /// A normal component (`SPROP_NORMAL`): sign and 11-bit fraction.
    pub fn bit_normal(&mut self) -> Result<f32> {
        let negative = self.bit()?;
        let v = self.bits(11)? as f32 / 2047.0;
        Ok(if negative { -v } else { v })
    }

    /// A vector of three `bit_coord`s, each present only if its flag is set.
    pub fn bit_vec_coord(&mut self) -> Result<[f32; 3]> {
        let (x, y, z) = (self.bit()?, self.bit()?, self.bit()?);
        let mut v = [0.0; 3];
        for (i, has) in [x, y, z].into_iter().enumerate() {
            if has {
                v[i] = self.bit_coord()?;
            }
        }
        Ok(v)
    }
}

/// Bits needed for values up to `n` (floor of log2, plus one), as the
/// engine sizes class ids and string table indices.
pub fn bits_for(n: u32) -> u32 {
    if n == 0 { 1 } else { 32 - n.leading_zeros() }
}

/// Floor of log2 (0 for 0).
pub fn log2(n: u32) -> u32 {
    31u32.saturating_sub(n.leading_zeros())
}

/// The writing side of `BitReader` (same bit order), for demos built in
/// code: tests, and the frames `write` makes (`usercmd`, `writer`).
#[derive(Clone, Debug, Default)]
pub struct BitWriter {
    data: Vec<u8>,
    pos: usize,
}

impl BitWriter {
    pub fn new() -> Self {
        Self::default()
    }

    /// Bits written so far.
    pub fn pos(&self) -> usize {
        self.pos
    }

    /// The low `n` bits of `v` (up to 32), least significant first.
    pub fn bits(&mut self, v: u32, n: u32) {
        for i in 0..n {
            if self.pos / 8 >= self.data.len() {
                self.data.push(0);
            }
            if (v >> i) & 1 == 1 {
                self.data[self.pos / 8] |= 1 << (self.pos % 8);
            }
            self.pos += 1;
        }
    }

    pub fn bit(&mut self, v: bool) {
        self.bits(v as u32, 1);
    }

    pub fn u8(&mut self, v: u8) {
        self.bits(v as u32, 8);
    }

    pub fn u16(&mut self, v: u16) {
        self.bits(v as u32, 16);
    }

    pub fn u32(&mut self, v: u32) {
        self.bits(v, 32);
    }

    pub fn i32(&mut self, v: i32) {
        self.bits(v as u32, 32);
    }

    pub fn f32(&mut self, v: f32) {
        self.bits(v.to_bits(), 32);
    }

    /// A NUL-terminated string.
    pub fn string(&mut self, s: &str) {
        for b in s.bytes() {
            self.u8(b);
        }
        self.u8(0);
    }

    /// The bytes (the last one padded with zero bits).
    pub fn finish(self) -> Vec<u8> {
        self.data
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    /// Packs (value, width) pairs least significant bit first, the way the
    /// engine writes them.
    pub fn pack(fields: &[(u32, u32)]) -> Vec<u8> {
        let mut out = Vec::new();
        let mut pos = 0usize;
        for &(v, n) in fields {
            for i in 0..n {
                if pos / 8 >= out.len() {
                    out.push(0u8);
                }
                if (v >> i) & 1 == 1 {
                    out[pos / 8] |= 1 << (pos % 8);
                }
                pos += 1;
            }
        }
        out
    }

    #[test]
    fn reads_across_byte_boundaries() {
        let data = pack(&[(5, 3), (0x1234, 13), (1, 1), (0xABCDEF01, 32), (0b10, 2)]);
        let mut r = BitReader::new(&data);
        assert_eq!(r.bits(3).unwrap(), 5);
        assert_eq!(r.bits(13).unwrap(), 0x1234);
        assert!(r.bit().unwrap());
        assert_eq!(r.u32().unwrap(), 0xABCDEF01);
        assert_eq!(r.bits(2).unwrap(), 0b10);
    }

    #[test]
    fn signed_and_strings() {
        let mut fields = vec![(0b111, 3)];
        fields.extend(b"hi\0".iter().map(|&b| (b as u32, 8)));
        let data = pack(&fields);
        let mut r = BitReader::new(&data);
        assert_eq!(r.signed(3).unwrap(), -1);
        assert_eq!(r.string().unwrap(), "hi");
    }

    #[test]
    fn ubitvar_and_varint() {
        // selector 1 (8 bits) then 200; varint 300 = 0xAC 0x02
        let data = pack(&[(1, 2), (200, 8), (0xAC, 8), (0x02, 8)]);
        let mut r = BitReader::new(&data);
        assert_eq!(r.ubitvar().unwrap(), 200);
        assert_eq!(r.var_u32().unwrap(), 300);
    }

    #[test]
    fn coords() {
        // has_int, has_frac, negative, int-1 = 99, frac = 16 -> -100.5
        let data = pack(&[(1, 1), (1, 1), (1, 1), (99, 14), (16, 5)]);
        assert_eq!(BitReader::new(&data).bit_coord().unwrap(), -100.5);
    }

    #[test]
    fn sub_reader_stops_at_its_window() {
        let data = pack(&[(0xFF, 8), (0x0F, 8)]);
        let mut r = BitReader::new(&data);
        let mut s = r.sub(4).unwrap();
        assert_eq!(s.bits(4).unwrap(), 0xF);
        assert!(s.bit().is_err());
        assert_eq!(r.bits(4).unwrap(), 0xF);
    }

    #[test]
    fn the_writer_writes_what_the_reader_reads() {
        let mut w = BitWriter::new();
        w.bits(5, 3);
        w.u32(0xABCD_EF01);
        w.bit(true);
        w.f32(-1.5);
        w.string("hi");
        assert_eq!(w.pos(), 3 + 32 + 1 + 32 + 24);
        let data = w.finish();
        assert_eq!(data, {
            let mut f = vec![(5, 3), (0xABCD_EF01, 32), (1, 1), ((-1.5f32).to_bits(), 32)];
            f.extend(b"hi\0".iter().map(|&b| (b as u32, 8)));
            pack(&f)
        });
        let mut r = BitReader::new(&data);
        assert_eq!(r.bits(3).unwrap(), 5);
        assert_eq!(r.u32().unwrap(), 0xABCD_EF01);
        assert!(r.bit().unwrap());
        assert_eq!(r.f32().unwrap(), -1.5);
        assert_eq!(r.string().unwrap(), "hi");
    }

    #[test]
    fn bit_counts() {
        assert_eq!(bits_for(1), 1);
        assert_eq!(bits_for(255), 8);
        assert_eq!(bits_for(256), 9);
        assert_eq!(log2(2048), 11);
    }
}
