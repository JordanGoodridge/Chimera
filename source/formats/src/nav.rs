//! Navigation mesh files (`maps/<map>.nav`, versions up to 16; CS:S ships
//! 9 and 16), per specs/cs_source/nav.md "File layout": the records as the
//! file has them, in Source units and axes. Linking areas into a mesh
//! (ids to indices, ladders, encounters) is the game's side
//! (`mashup::games::cs_source::nav`).

use glam::Vec3;

const MAGIC: u32 = 0xFEED_FACE;
/// Runtime-only flag bits that should never be in a file.
const RUNTIME_FLAGS: u32 = 0xE000_0000;

/// What the parser saw beyond the mesh itself, for tests and diagnostics.
#[derive(Clone, Debug, Default)]
pub struct NavInfo {
    pub version: u32,
    pub subversion: u32,
    pub hiding_spots: usize,
    pub encounters: usize,
    /// Bytes left after the last record (0 for a well-formed file).
    pub trailing: usize,
}

/// One area as stored.
#[derive(Clone, Debug, Default)]
pub struct Area {
    pub id: u32,
    /// Attribute flags, runtime-only bits cleared.
    pub flags: u32,
    /// North-west and south-east corners (Source y grows south).
    pub nw: Vec3,
    pub se: Vec3,
    /// Heights of the other two corners.
    pub ne_z: f32,
    pub sw_z: f32,
    /// Connected area ids per compass side (north, east, south, west).
    pub links: [Vec<u32>; 4],
    /// Place index, 1-based (0: none).
    pub place: u16,
    /// Ladder ids going up, then down (duplicates removed).
    pub ladders: [Vec<u32>; 2],
    /// Hiding spots: (id, position, flags).
    pub spots: Vec<(u32, Vec3, u8)>,
    /// Encounter paths: (from area, to area, [(spot id, t of 255)]).
    pub encounters: Vec<(u32, u32, Vec<(u32, u8)>)>,
    /// Potentially visible areas: (area id, attributes).
    pub visible: Vec<(u32, u8)>,
    /// The area whose visible list this one inherits (0: none).
    pub inherit: u32,
}

/// One ladder as stored.
#[derive(Clone, Debug)]
pub struct Ladder {
    pub id: u32,
    pub top: Vec3,
    pub bottom: Vec3,
    /// Units.
    pub length: f32,
    /// The compass direction it faces (north, east, south, west).
    pub dir: u32,
    /// Area ids at its top (forward, left, right) and its bottom.
    pub top_areas: [u32; 3],
    pub bottom_area: u32,
}

/// A whole `.nav` file.
#[derive(Clone, Debug, Default)]
pub struct NavFile {
    pub info: NavInfo,
    /// Place names, in order (an area's `place` is 1-based into this).
    pub places: Vec<String>,
    pub areas: Vec<Area>,
    pub ladders: Vec<Ladder>,
}

struct Reader<'a> {
    b: &'a [u8],
    at: usize,
}

impl Reader<'_> {
    fn take(&mut self, n: usize) -> Result<&[u8], String> {
        let s = self
            .b
            .get(self.at..self.at + n)
            .ok_or_else(|| format!("nav: truncated at byte {}", self.at))?;
        self.at += n;
        Ok(s)
    }
    fn u8(&mut self) -> Result<u8, String> {
        Ok(self.take(1)?[0])
    }
    fn u16(&mut self) -> Result<u16, String> {
        Ok(u16::from_le_bytes(self.take(2)?.try_into().unwrap()))
    }
    fn u32(&mut self) -> Result<u32, String> {
        Ok(u32::from_le_bytes(self.take(4)?.try_into().unwrap()))
    }
    fn f32(&mut self) -> Result<f32, String> {
        Ok(f32::from_le_bytes(self.take(4)?.try_into().unwrap()))
    }
    fn vec3(&mut self) -> Result<Vec3, String> {
        Ok(Vec3::new(self.f32()?, self.f32()?, self.f32()?))
    }
}

/// Read a `.nav` file.
pub fn parse(bytes: &[u8]) -> Result<NavFile, String> {
    let mut r = Reader { b: bytes, at: 0 };
    if r.u32()? != MAGIC {
        return Err("nav: bad magic".into());
    }
    let v = r.u32()?;
    if v > 16 {
        return Err(format!("nav: version {v} is newer than 16"));
    }
    let mut info = NavInfo {
        version: v,
        ..Default::default()
    };
    if v >= 10 {
        info.subversion = r.u32()?;
    }
    if v >= 4 {
        r.u32()?; // size of the BSP it was built for (advisory)
    }
    if v >= 14 {
        r.u8()?; // analysed
    }
    let mut places = Vec::new();
    if v >= 5 {
        for _ in 0..r.u16()? {
            let len = r.u16()? as usize;
            let raw = r.take(len)?;
            let end = raw.iter().position(|&c| c == 0).unwrap_or(raw.len());
            places.push(String::from_utf8_lossy(&raw[..end]).into_owned());
        }
    }
    if v >= 12 {
        r.u8()?; // has unnamed areas
    }

    let count = r.u32()? as usize;
    let mut areas = Vec::with_capacity(count);
    for _ in 0..count {
        let id = r.u32()?;
        let flags = match v {
            ..=8 => r.u8()? as u32,
            9..=12 => r.u16()? as u32,
            _ => r.u32()?,
        } & !RUNTIME_FLAGS;
        let (nw, se) = (r.vec3()?, r.vec3()?);
        let (ne_z, sw_z) = (r.f32()?, r.f32()?);
        let mut links: [Vec<u32>; 4] = Default::default();
        for side in &mut links {
            for _ in 0..r.u32()? {
                side.push(r.u32()?);
            }
        }
        let count = r.u8()? as usize;
        info.hiding_spots += count;
        let mut spots = Vec::with_capacity(count);
        for _ in 0..count {
            if v >= 2 {
                let id = r.u32()?;
                let pos = r.vec3()?;
                spots.push((id, pos, r.u8()?));
            } else {
                spots.push((u32::MAX, r.vec3()?, 0));
            }
        }
        if v < 15 {
            let approaches = r.u8()? as usize;
            r.take(approaches * 14)?;
        }
        let count = r.u32()? as usize;
        info.encounters += count;
        let mut encounters = Vec::new();
        if v >= 3 {
            for _ in 0..count {
                let from = r.u32()?;
                r.u8()?; // from_dir
                let to = r.u32()?;
                r.u8()?; // to_dir
                let n = r.u8()? as usize;
                let mut list = Vec::with_capacity(n);
                for _ in 0..n {
                    let id = r.u32()?;
                    list.push((id, r.u8()?));
                }
                encounters.push((from, to, list));
            }
        } else if count > 0 {
            return Err("nav: version < 3 encounters unsupported".into());
        }
        let place = if v >= 5 { r.u16()? } else { 0 };
        let mut ladders: [Vec<u32>; 2] = Default::default();
        if v >= 7 {
            for dir in &mut ladders {
                for _ in 0..r.u32()? {
                    let l = r.u32()?;
                    if !dir.contains(&l) {
                        dir.push(l);
                    }
                }
            }
        }
        if v >= 8 {
            r.take(8)?; // earliest occupy times
        }
        if v >= 11 {
            r.take(16)?; // corner light
        }
        let mut visible = Vec::new();
        let mut inherit = 0;
        if v >= 16 {
            let n = r.u32()? as usize;
            visible.reserve(n);
            for _ in 0..n {
                let id = r.u32()?;
                visible.push((id, r.u8()?));
            }
            inherit = r.u32()?;
        }
        if v >= 10 && info.subversion >= 1 {
            // CS:S's own per-area data: a count, 0 in every shipped file
            // (spec open question 1: entry size assumed 14).
            let n = r.u8()? as usize;
            r.take(n * 14)?;
        }
        areas.push(Area {
            id,
            flags,
            nw,
            se,
            ne_z,
            sw_z,
            links,
            place,
            ladders,
            spots,
            encounters,
            visible,
            inherit,
        });
    }

    let mut ladders = Vec::new();
    if v >= 6 {
        for _ in 0..r.u32()? {
            let id = r.u32()?;
            let _width = r.f32()?;
            let (top, bottom) = (r.vec3()?, r.vec3()?);
            let length = r.f32()?;
            let dir = r.u32()?;
            if v == 6 {
                r.u8()?;
            }
            let (forward, left, right, _behind, bottom_area) = (r.u32()?, r.u32()?, r.u32()?, r.u32()?, r.u32()?);
            ladders.push(Ladder {
                id,
                top,
                bottom,
                length,
                dir,
                top_areas: [forward, left, right],
                bottom_area,
            });
        }
    }
    info.trailing = bytes.len() - r.at;
    Ok(NavFile {
        info,
        places,
        areas,
        ladders,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A version 9 file: two linked areas, one hiding spot, no ladders.
    #[test]
    fn reads_a_small_file() {
        let mut b = Vec::new();
        let u32s = |b: &mut Vec<u8>, v: &[u32]| v.iter().for_each(|x| b.extend(x.to_le_bytes()));
        let f32s = |b: &mut Vec<u8>, v: &[f32]| v.iter().for_each(|x| b.extend(x.to_le_bytes()));
        u32s(&mut b, &[MAGIC, 9, 1234]);
        b.extend(1u16.to_le_bytes());
        b.extend(4u16.to_le_bytes());
        b.extend(b"Mid\0");
        u32s(&mut b, &[2]);
        for (id, other) in [(1u32, 2u32), (2, 1)] {
            u32s(&mut b, &[id]);
            b.extend(1u16.to_le_bytes());
            f32s(&mut b, &[0.0, 0.0, 8.0, 64.0, 64.0, 8.0, 9.0, 7.0]);
            // East links to the other area; other sides empty.
            u32s(&mut b, &[0, 1, other, 0, 0]);
            if id == 1 {
                b.push(1);
                u32s(&mut b, &[77]);
                f32s(&mut b, &[32.0, 32.0, 8.0]);
                b.push(2);
            } else {
                b.push(0);
            }
            b.push(0); // approach spots
            u32s(&mut b, &[0]); // encounters
            b.extend(1u16.to_le_bytes()); // place
            u32s(&mut b, &[0, 0]); // ladders up, down
            b.extend([0u8; 8]); // occupy times
        }
        u32s(&mut b, &[0]); // ladders
        let nav = parse(&b).unwrap();
        assert_eq!((nav.info.version, nav.info.hiding_spots, nav.info.trailing), (9, 1, 0));
        assert_eq!(nav.places, ["Mid"]);
        assert_eq!(nav.areas.len(), 2);
        let a = &nav.areas[0];
        assert_eq!((a.id, a.flags, a.place), (1, 1, 1));
        assert_eq!(a.links[1], [2]);
        assert_eq!(a.se, Vec3::new(64.0, 64.0, 8.0));
        assert_eq!((a.ne_z, a.sw_z), (9.0, 7.0));
        assert_eq!(a.spots, [(77, Vec3::new(32.0, 32.0, 8.0), 2)]);
        assert!(parse(b"nope").is_err());
    }
}
