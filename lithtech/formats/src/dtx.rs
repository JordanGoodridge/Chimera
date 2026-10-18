//! LithTech textures (`.dtx`) as Combat Arms ships them, version −5, per
//! specs/combat_arms/textures_dtx.md (section names below are the
//! spec's): a 164-byte header, then each mipmap level's pixels, largest
//! first ("Mipmap chain"), in 32-bit BGRA or DXT1/3/5 ("Format codes and
//! pixel layouts").
//!
//! `Dtx::parse` checks the header and that every level fits the file;
//! `Dtx::rgba` decodes a level to RGBA8 (rows from the top). `command`
//! holds the command string and `alpha_ref` reads its alpha test.
//!
//! Known shortcuts: only what drawing a world needs. Not modelled here:
//! the load-time mip skipping by texture group ("Load-time behaviour"),
//! the 16-bit preferences, sections after the pixels (none in the
//! install; skipped), and resolving `detailtex`/`envmap` links (the
//! command string is kept for that). Interpolated DXT colours round to
//! nearest ("Colour block").

use std::fmt;

/// "Header": its length; level 0 starts here.
pub const HEADER_LEN: usize = 164;
/// "Header": the only file version.
pub const VERSION: i32 = -5;

/// Pixel formats the install uses ("Format codes and pixel layouts").
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Format {
    /// Code 3 (and 0): B, G, R, A bytes.
    Bgra32,
    /// Code 4.
    Dxt1,
    /// Code 5.
    Dxt3,
    /// Code 6.
    Dxt5,
}

impl Format {
    /// S(f, w, h): a level's bytes.
    pub fn level_size(self, w: u32, h: u32) -> usize {
        let n = w as usize * h as usize;
        match self {
            Format::Bgra32 => 4 * n,
            Format::Dxt1 => n / 2,
            Format::Dxt3 | Format::Dxt5 => n,
        }
    }
}

/// Why a `.dtx` was rejected.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DtxError(pub String);

impl fmt::Display for DtxError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "dtx: {}", self.0)
    }
}

impl std::error::Error for DtxError {}

/// A parsed texture: header fields and the file's bytes.
#[derive(Clone, Debug, PartialEq)]
pub struct Dtx {
    pub width: u32,
    pub height: u32,
    /// N, levels stored.
    pub mip_count: u32,
    pub flags: u32,
    pub surface_flags: u32,
    pub group: u8,
    /// Mip levels to use (0 means 8).
    pub use_levels: u8,
    pub format: Format,
    /// The command string (up to its first 0x00).
    pub command: String,
    /// Where each level starts, and the end of the pixels.
    pub offsets: Vec<usize>,
    data: Vec<u8>,
}

/// Texture flag bit 0: alpha is a fullbright mask.
pub const FLAG_FULLBRIGHT: u32 = 0x1;

impl Dtx {
    pub fn parse(data: Vec<u8>) -> Result<Dtx, DtxError> {
        if data.len() < HEADER_LEN {
            return Err(DtxError(format!("{} bytes: too short for a texture", data.len())));
        }
        let u16_at = |o: usize| u16::from_le_bytes([data[o], data[o + 1]]) as u32;
        let u32_at = |o: usize| u32::from_le_bytes(data[o..o + 4].try_into().unwrap());
        if u32_at(0) != 0 {
            return Err(DtxError(format!("resource type {}, not a texture", u32_at(0))));
        }
        let version = u32_at(8) as i32;
        if version != VERSION {
            return Err(DtxError(format!("version {version}, not {VERSION}")));
        }
        let (width, height, mip_count) = (u16_at(4), u16_at(6), u16_at(0x0C));
        if !(1..=8).contains(&mip_count) {
            return Err(DtxError(format!("{mip_count} mip levels (1..8)")));
        }
        let format = match data[0x1A] {
            0 | 3 => Format::Bgra32,
            4 => Format::Dxt1,
            5 => Format::Dxt3,
            6 => Format::Dxt5,
            c => return Err(DtxError(format!("format code {c} is not supported"))),
        };
        let mut offsets = vec![HEADER_LEN];
        for i in 0..mip_count {
            let (w, h) = (width >> i, height >> i);
            if format != Format::Bgra32 && (w < 4 || h < 4) {
                return Err(DtxError(format!("level {i} is {w} x {h}, under a compressed block")));
            }
            let end = offsets.last().unwrap() + format.level_size(w, h);
            offsets.push(end);
        }
        let end = *offsets.last().unwrap();
        if data.len() < end {
            return Err(DtxError(format!("{} bytes, the levels end at {end}", data.len())));
        }
        let field = &data[0x24..0x24 + 128];
        let command = field[..field.iter().position(|&b| b == 0).unwrap_or(128)]
            .iter()
            .map(|&b| b as char)
            .collect();
        Ok(Dtx {
            width,
            height,
            mip_count,
            flags: u32_at(0x10),
            surface_flags: u32_at(0x14),
            group: data[0x18],
            use_levels: data[0x19],
            format,
            command,
            offsets,
            data,
        })
    }

    /// Level `i`'s size.
    pub fn level_dims(&self, i: u32) -> (u32, u32) {
        (self.width >> i, self.height >> i)
    }

    /// Level `i`'s stored bytes.
    pub fn level_bytes(&self, i: u32) -> &[u8] {
        &self.data[self.offsets[i as usize]..self.offsets[i as usize + 1]]
    }

    /// The alpha test's reference from the command string (`alpharef N`,
    /// keyword in any case; N = 0 or missing: no test).
    pub fn alpha_ref(&self) -> Option<u8> {
        self.command.split(';').find_map(|stmt| {
            let mut words = stmt.split_whitespace();
            if !words.next()?.eq_ignore_ascii_case("alpharef") {
                return None;
            }
            let n: u32 = words.next()?.parse().ok()?;
            (n > 0).then(|| n.min(255) as u8)
        })
    }

    /// Level `i` as RGBA8, rows from the top.
    pub fn rgba(&self, i: u32) -> Vec<u8> {
        let (w, h) = self.level_dims(i);
        decode(self.format, w, h, self.level_bytes(i))
    }
}

/// A level's bytes in `format` as RGBA8.
pub fn decode(format: Format, w: u32, h: u32, src: &[u8]) -> Vec<u8> {
    let (w, h) = (w as usize, h as usize);
    let mut out = vec![0u8; w * h * 4];
    if format == Format::Bgra32 {
        for (o, p) in out.as_chunks_mut::<4>().0.iter_mut().zip(src.as_chunks::<4>().0) {
            *o = [p[2], p[1], p[0], p[3]];
        }
        return out;
    }
    let block = if format == Format::Dxt1 { 8 } else { 16 };
    let (bw, bh) = (w.div_ceil(4), h.div_ceil(4));
    for by in 0..bh {
        for bx in 0..bw {
            let b = &src[(by * bw + bx) * block..][..block];
            let (alpha, colour) = if format == Format::Dxt1 { (None, b) } else { (Some(&b[..8]), &b[8..]) };
            let mut texels = colour_block(colour, format == Format::Dxt1);
            match (format, alpha) {
                (Format::Dxt3, Some(a)) => {
                    for (t, px) in texels.iter_mut().enumerate() {
                        let nibble = (a[t / 2] >> (4 * (t % 2))) & 0xF;
                        px[3] = nibble * 17;
                    }
                }
                (Format::Dxt5, Some(a)) => {
                    let table = dxt5_alphas(a[0], a[1]);
                    let bits = a[2..8].iter().rev().fold(0u64, |acc, &x| (acc << 8) | x as u64);
                    for (t, px) in texels.iter_mut().enumerate() {
                        px[3] = table[((bits >> (3 * t)) & 7) as usize];
                    }
                }
                _ => {}
            }
            for (t, px) in texels.iter().enumerate() {
                let (x, y) = (bx * 4 + t % 4, by * 4 + t / 4);
                if x < w && y < h {
                    out[(y * w + x) * 4..][..4].copy_from_slice(px);
                }
            }
        }
    }
    out
}

/// A colour block's 16 texels ("Colour block").
fn colour_block(b: &[u8], dxt1: bool) -> [[u8; 4]; 16] {
    let c0 = u16::from_le_bytes([b[0], b[1]]);
    let c1 = u16::from_le_bytes([b[2], b[3]]);
    let widen = |c: u16| {
        let (r, g, bl) = ((c >> 11) & 31, (c >> 5) & 63, c & 31);
        [(r << 3 | r >> 2) as u32, (g << 2 | g >> 4) as u32, (bl << 3 | bl >> 2) as u32]
    };
    let (a, z) = (widen(c0), widen(c1));
    let mix = |m: u32, n: u32, d: u32| -> [u8; 4] {
        let ch = |k: usize| ((m * a[k] + n * z[k] + d / 2) / d) as u8;
        [ch(0), ch(1), ch(2), 255]
    };
    let palette = if !dxt1 || c0 > c1 {
        [mix(1, 0, 1), mix(0, 1, 1), mix(2, 1, 3), mix(1, 2, 3)]
    } else {
        [mix(1, 0, 1), mix(0, 1, 1), mix(1, 1, 2), [0, 0, 0, 0]]
    };
    let idx = u32::from_le_bytes([b[4], b[5], b[6], b[7]]);
    std::array::from_fn(|t| palette[((idx >> (2 * t)) & 3) as usize])
}

/// "DXT5 alpha": the eight alphas from a0 and a1.
fn dxt5_alphas(a0: u8, a1: u8) -> [u8; 8] {
    let (a, b) = (a0 as u32, a1 as u32);
    let mut t = [a0, a1, 0, 0, 0, 0, 0, 0];
    if a > b {
        for (k, x) in t.iter_mut().enumerate().skip(2) {
            let k = k as u32;
            *x = (((8 - k) * a + (k - 1) * b + 3) / 7) as u8;
        }
    } else {
        for (k, x) in t.iter_mut().enumerate().take(6).skip(2) {
            let k = k as u32;
            *x = (((6 - k) * a + (k - 1) * b + 2) / 5) as u8;
        }
        t[6] = 0;
        t[7] = 255;
    }
    t
}
