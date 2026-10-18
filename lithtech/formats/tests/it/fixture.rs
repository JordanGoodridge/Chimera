//! Tiny `.rez` archives written in code by the writer rules in
//! specs/combat_arms/rez_archive.md (no game data): file data from 0xEC,
//! then directory blocks depth-first with a zero gap after each but the
//! root, which comes last; a header that is not zero-filled, so leftover
//! bytes must be ignored.

/// A folder: its name, subfolders and files (name, extension, stored bytes).
#[derive(Default)]
pub struct Dir {
    pub name: String,
    pub dirs: Vec<Dir>,
    pub files: Vec<(String, String, Vec<u8>)>,
}

impl Dir {
    pub fn new(name: &str) -> Self {
        Self {
            name: name.into(),
            ..Default::default()
        }
    }

    pub fn file(mut self, name: &str, ext: &str, data: impl Into<Vec<u8>>) -> Self {
        self.files.push((name.into(), ext.into(), data.into()));
        self
    }

    pub fn dir(mut self, dir: Dir) -> Self {
        self.dirs.push(dir);
        self
    }
}

fn u32le(v: u32) -> [u8; 4] {
    v.to_le_bytes()
}

const TIME: u32 = 1_700_000_000;

/// Write `dir`'s file data, then its blocks; returns its block's (offset,
/// size).
fn write_dir(out: &mut Vec<u8>, dir: &Dir, root: bool) -> (u32, u32) {
    let mut records = Vec::new();
    for sub in &dir.dirs {
        let (offset, size) = write_dir(out, sub, false);
        records.extend(u32le(1));
        records.extend(u32le(offset));
        records.extend(u32le(size));
        records.extend(u32le(TIME));
        records.extend(sub.name.as_bytes());
        records.push(0);
    }
    for (name, ext, data) in &dir.files {
        // Data lives before the directory area; files here go at the end
        // of what is written so far, which is fine for a reader that
        // locates everything by offset.
        let offset = out.len() as u32;
        out.extend_from_slice(data);
        records.extend(u32le(0));
        records.extend(u32le(if data.is_empty() { 0 } else { offset }));
        records.extend(u32le(data.len() as u32));
        records.extend(u32le(TIME));
        records.extend(u32le(0)); // id
        records.extend(u32le(lithtech_formats::rez::encode_type(ext)));
        records.extend(u32le(0)); // key count
        records.extend(name.to_uppercase().as_bytes());
        records.push(0);
        records.push(0); // empty description
    }
    let offset = out.len() as u32;
    out.extend_from_slice(&records);
    if !root {
        // "Edge cases": zero-filled gaps after every block but the root.
        out.extend(std::iter::repeat_n(0u8, records.len()));
    }
    (offset, records.len() as u32)
}

/// An archive titled `title` whose root block holds `top` (one folder, as
/// in every observed archive).
pub fn build(title: &str, top: Dir) -> Vec<u8> {
    let mut out = vec![0u8; 0xEC];
    let root = Dir::new("").dir(top);
    let (root_offset, root_size) = write_dir(&mut out, &root, true);
    // Data and blocks interleave here (each folder's data before its
    // block), so "next write" is only summed into the check: the root's
    // offset stands in for it.
    let next_write = root_offset;

    let h = &mut out[..0xEC];
    h.fill(0x5A);
    h[0x00] = 0x26;
    h[0x01] = 0x23;
    h[0x02..0x3E].fill(b' ');
    let banner = b"RezMgr Version 1 Copyright (C) 1995 MONOLITH INC.";
    h[0x02..0x02 + banner.len()].copy_from_slice(banner);
    h[0x3E] = 0x21;
    h[0x3F] = 0x22;
    h[0x40..0x7C].fill(b' ');
    h[0x40..0x40 + title.len()].copy_from_slice(title.as_bytes());
    h[0x7C] = 0x25;
    h[0x7D] = 0x27;
    h[0x7E] = 0x2A;
    let s = root_offset
        .wrapping_add(root_size)
        .wrapping_add(0)
        .wrapping_add(next_write);
    put_number(h, 0xA2, s);
    put_number(h, 0x80, s ^ 23_807_011);
    h[0xA1] = 0x10;
    h[0x7F] = 0x10 ^ 0x11;
    h[0xC2] = 0x01;
    h[0xA0] = 0x01 ^ 0x11;
    for (at, v) in [
        (0xC3, 1),
        (0xC7, root_offset),
        (0xCB, root_size),
        (0xCF, 0),
        (0xD3, next_write),
    ] {
        h[at..at + 4].copy_from_slice(&u32le(v));
    }
    for at in [0xD7, 0xDB, 0xDF, 0xE3, 0xE7] {
        h[at..at + 4].copy_from_slice(&u32le(0));
    }
    h[0xEB] = 1;
    out
}

/// A check number as signed decimal and a 0x00, the rest of its area
/// left as it was.
pub fn put_number(h: &mut [u8], at: usize, v: u32) {
    let text = format!("{}", v as i32);
    h[at..at + text.len()].copy_from_slice(text.as_bytes());
    h[at + text.len()] = 0;
}
