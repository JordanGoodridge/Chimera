//! One bounds-checked little-endian reader for the format crates
//! (`source_formats`' `.nav`, `.pcf`, VPK; `lithtech_formats`' `.ltb`,
//! world): `Le` walks a byte slice and gives `Truncated` instead of
//! panicking. A format wraps its own error around `Truncated` (`From`).

use std::{fmt, io};

/// A read ran past the end of the data.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Truncated {
    /// Offset the read started at.
    pub at: usize,
    pub wanted: usize,
    pub left: usize,
}

impl fmt::Display for Truncated {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} bytes wanted, {} left (at byte {})", self.wanted, self.left, self.at)
    }
}

impl std::error::Error for Truncated {}

impl From<Truncated> for String {
    fn from(t: Truncated) -> String {
        t.to_string()
    }
}

impl From<Truncated> for io::Error {
    fn from(t: Truncated) -> io::Error {
        io::Error::new(io::ErrorKind::InvalidData, t.to_string())
    }
}

/// A bounds-checked little-endian reader over a slice.
#[derive(Clone, Debug)]
pub struct Le<'a> {
    /// The whole slice (not only what is left).
    pub b: &'a [u8],
    /// Offset of the next read.
    pub pos: usize,
}

impl<'a> Le<'a> {
    pub fn new(b: &'a [u8]) -> Self {
        Le { b, pos: 0 }
    }

    /// Bytes not read yet.
    pub fn left(&self) -> usize {
        self.b.len() - self.pos
    }

    pub fn take(&mut self, n: usize) -> Result<&'a [u8], Truncated> {
        let end = self.pos.checked_add(n).filter(|&e| e <= self.b.len()).ok_or(Truncated {
            at: self.pos,
            wanted: n,
            left: self.left(),
        })?;
        let s = &self.b[self.pos..end];
        self.pos = end;
        Ok(s)
    }

    pub fn array<const N: usize>(&mut self) -> Result<[u8; N], Truncated> {
        Ok(self.take(N)?.try_into().unwrap())
    }

    pub fn u8(&mut self) -> Result<u8, Truncated> {
        Ok(self.take(1)?[0])
    }

    pub fn u16(&mut self) -> Result<u16, Truncated> {
        Ok(u16::from_le_bytes(self.array()?))
    }

    pub fn i16(&mut self) -> Result<i16, Truncated> {
        Ok(i16::from_le_bytes(self.array()?))
    }

    pub fn u32(&mut self) -> Result<u32, Truncated> {
        Ok(u32::from_le_bytes(self.array()?))
    }

    pub fn i32(&mut self) -> Result<i32, Truncated> {
        Ok(i32::from_le_bytes(self.array()?))
    }

    pub fn f32(&mut self) -> Result<f32, Truncated> {
        Ok(f32::from_le_bytes(self.array()?))
    }

    /// `N` floats in a row.
    pub fn f32s<const N: usize>(&mut self) -> Result<[f32; N], Truncated> {
        let mut out = [0.0; N];
        for v in &mut out {
            *v = self.f32()?;
        }
        Ok(out)
    }

    /// A NUL-terminated string (lossy UTF-8); `None` without a NUL.
    pub fn cstr(&mut self) -> Option<String> {
        let rest = &self.b[self.pos..];
        let end = rest.iter().position(|&c| c == 0)?;
        let s = String::from_utf8_lossy(&rest[..end]).into_owned();
        self.pos += end + 1;
        Some(s)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_little_endian_and_refuses_to_overrun() {
        let mut r = Le::new(&[1, 2, 3, 4, 5, 0xff, 0xff, b'h', b'i', 0, 9]);
        assert_eq!(r.u32().unwrap(), 0x0403_0201);
        assert_eq!(r.u8().unwrap(), 5);
        assert_eq!(r.i16().unwrap(), -1);
        assert_eq!(r.cstr().as_deref(), Some("hi"));
        assert_eq!(r.left(), 1);
        let e = r.u16().unwrap_err();
        assert_eq!((e.at, e.wanted, e.left), (10, 2, 1));
        assert_eq!(r.pos, 10, "a failed read doesn't move");
        assert_eq!(r.cstr(), None);
    }

    #[test]
    fn floats_in_a_row() {
        let mut b = Vec::new();
        for v in [1.0f32, 2.0, 3.0] {
            b.extend_from_slice(&v.to_le_bytes());
        }
        assert_eq!(Le::new(&b).f32s::<3>().unwrap(), [1.0, 2.0, 3.0]);
    }
}
