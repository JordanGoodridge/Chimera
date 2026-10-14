//! Detail props: the BSP's `dprp` game lump (public BSP description,
//! "Detail props"), the grass, weeds and small bushes vbsp scatters from
//! the map's detail.vbsp. What they become is the game's side
//! (`mashup::games::cs_source::detail`).
//!
//! The lump (version 4): a model dictionary (count, then 128-byte names),
//! a sprite dictionary (count, then per sprite its quad's upper-left and
//! lower-right corners in units, x across and y up, and the same corners'
//! texture coordinates), then the objects (count, 52 bytes each): origin,
//! angles, model or sprite index, leaf, lighting (`ColorRGBExp32`), light
//! styles and their count, sway amount, shape angle, shape size,
//! orientation, three pad bytes, type, three pad bytes, scale.

use glam::{Vec2, Vec3};

/// The game lump directory entry: id, flags, version, offset, length.
const GAME_LUMP: usize = 35;
const OBJECT_SIZE: usize = 52;
/// Object types.
pub const TYPE_MODEL: u8 = 0;
pub const TYPE_SPRITE: u8 = 1;
pub const TYPE_CROSS: u8 = 2;
pub const TYPE_TRI: u8 = 3;
/// Sprite orientations: fixed by its angles, facing the view, turning
/// only about the vertical to face it.
pub const ORIENT_SCREEN: u8 = 1;
pub const ORIENT_SCREEN_VERTICAL: u8 = 2;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SpriteDict {
    /// Upper-left and lower-right corners, units (x across, y up).
    pub ul: Vec2,
    pub lr: Vec2,
    /// Their texture coordinates.
    pub tex_ul: Vec2,
    pub tex_lr: Vec2,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DetailObject {
    /// Source units and degrees.
    pub origin: Vec3,
    pub angles: Vec3,
    /// Model or sprite dictionary index.
    pub index: u16,
    /// Linear lightmap units.
    pub light: [f32; 3],
    pub sway: u8,
    pub shape_angle: u8,
    pub shape_size: u8,
    pub orientation: u8,
    pub kind: u8,
    pub scale: f32,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct DetailLump {
    pub models: Vec<String>,
    pub sprites: Vec<SpriteDict>,
    pub objects: Vec<DetailObject>,
}

fn u32_at(b: &[u8], at: usize) -> Option<u32> {
    Some(u32::from_le_bytes(b.get(at..at + 4)?.try_into().ok()?))
}

fn f32_at(b: &[u8], at: usize) -> Option<f32> {
    u32_at(b, at).map(f32::from_bits)
}

fn v2(b: &[u8], at: usize) -> Option<Vec2> {
    Some(Vec2::new(f32_at(b, at)?, f32_at(b, at + 4)?))
}

fn v3(b: &[u8], at: usize) -> Option<Vec3> {
    Some(Vec3::new(f32_at(b, at)?, f32_at(b, at + 4)?, f32_at(b, at + 8)?))
}

/// The `dprp` game lump's bytes (decompressed when the map stores it
/// LZMA-compressed) and its version, from a BSP file whose own lumps are
/// plain (`lumps::inflate`). Its sub-lumps carry absolute file offsets.
fn detail_lump_bytes(bsp: &[u8]) -> Option<(Vec<u8>, u16)> {
    let at = 8 + GAME_LUMP * 16;
    let ofs = u32_at(bsp, at)? as usize;
    let len = u32_at(bsp, at + 4)? as usize;
    let lump = bsp.get(ofs..ofs + len)?;
    let count = u32_at(lump, 0)? as usize;
    let entry = |i: usize| -> Option<(u32, u16, u16, usize, usize)> {
        let e = lump.get(4 + i * 16..4 + i * 16 + 16)?;
        Some((
            u32::from_le_bytes(e[0..4].try_into().ok()?),
            u16::from_le_bytes(e[4..6].try_into().ok()?),
            u16::from_le_bytes(e[6..8].try_into().ok()?),
            u32::from_le_bytes(e[8..12].try_into().ok()?) as usize,
            u32::from_le_bytes(e[12..16].try_into().ok()?) as usize,
        ))
    };
    for i in 0..count.min(64) {
        let (id, flags, version, ofs, len) = entry(i)?;
        if id != u32::from_be_bytes(*b"dprp") {
            continue;
        }
        if flags & 1 != 0 {
            // Compressed: its data runs to the next sub-lump's offset.
            let end = entry(i + 1).map(|e| e.3)?;
            let raw = bsp.get(ofs..end)?;
            return Some((super::lumps::decompress(raw)?, version));
        }
        return Some((bsp.get(ofs..ofs + len)?.to_vec(), version));
    }
    None
}

/// The detail lump of a map, if it has one this reader knows (version 4,
/// CS:S's).
pub fn read(bsp: &[u8]) -> Option<DetailLump> {
    let (b, version) = detail_lump_bytes(bsp)?;
    if version != 4 {
        return None;
    }
    let mut out = DetailLump::default();
    let mut p = 0;
    let models = u32_at(&b, p)? as usize;
    p += 4;
    for k in 0..models.min(4096) {
        let name = b.get(p + k * 128..p + k * 128 + 128)?;
        let end = name.iter().position(|&c| c == 0).unwrap_or(128);
        out.models
            .push(String::from_utf8_lossy(&name[..end]).to_lowercase().replace('\\', "/"));
    }
    p += models * 128;
    let sprites = u32_at(&b, p)? as usize;
    p += 4;
    for k in 0..sprites.min(4096) {
        let s = p + k * 32;
        out.sprites.push(SpriteDict {
            ul: v2(&b, s)?,
            lr: v2(&b, s + 8)?,
            tex_ul: v2(&b, s + 16)?,
            tex_lr: v2(&b, s + 24)?,
        });
    }
    p += sprites * 32;
    let objects = u32_at(&b, p)? as usize;
    p += 4;
    for k in 0..objects.min(1 << 20) {
        let o = p + k * OBJECT_SIZE;
        let rgbe = b.get(o + 28..o + 32)?;
        let scale = 2f32.powi(rgbe[3] as i8 as i32) / 255.0;
        out.objects.push(DetailObject {
            origin: v3(&b, o)?,
            angles: v3(&b, o + 12)?,
            index: u16::from_le_bytes(b.get(o + 24..o + 26)?.try_into().ok()?),
            light: [rgbe[0] as f32 * scale, rgbe[1] as f32 * scale, rgbe[2] as f32 * scale],
            sway: *b.get(o + 37)?,
            shape_angle: *b.get(o + 38)?,
            shape_size: *b.get(o + 39)?,
            orientation: *b.get(o + 40)?,
            kind: *b.get(o + 44)?,
            scale: f32_at(&b, o + 48)?,
        });
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lump_bytes(objects: &[(u8, u8, u16)]) -> Vec<u8> {
        let mut b = Vec::new();
        b.extend_from_slice(&1u32.to_le_bytes());
        let mut name = b"models/props/bush.mdl".to_vec();
        name.resize(128, 0);
        b.extend_from_slice(&name);
        b.extend_from_slice(&1u32.to_le_bytes());
        for f in [-6.0f32, 12.0, 6.0, 0.0, 0.5, 0.25, 0.75, 0.5] {
            b.extend_from_slice(&f.to_le_bytes());
        }
        b.extend_from_slice(&(objects.len() as u32).to_le_bytes());
        for &(kind, orientation, index) in objects {
            let mut o = Vec::new();
            for f in [10.0f32, 20.0, 30.0, 0.0, 90.0, 0.0] {
                o.extend_from_slice(&f.to_le_bytes());
            }
            o.extend_from_slice(&index.to_le_bytes());
            o.extend_from_slice(&7u16.to_le_bytes());
            o.extend_from_slice(&[128, 64, 32, 1]);
            o.extend_from_slice(&0u32.to_le_bytes());
            o.extend_from_slice(&[0, 127, 30, 25, orientation, 0, 0, 0, kind, 0, 0, 0]);
            o.extend_from_slice(&2.0f32.to_le_bytes());
            assert_eq!(o.len(), OBJECT_SIZE);
            b.extend_from_slice(&o);
        }
        b
    }

    /// A BSP holding only a game lump with `dprp`.
    fn bsp_with(dprp: &[u8]) -> Vec<u8> {
        let header = 8 + 64 * 16 + 4;
        let mut file = vec![0u8; header];
        let game_at = file.len();
        let sub = game_at + 4 + 16;
        file.extend_from_slice(&1u32.to_le_bytes());
        file.extend_from_slice(b"prpd");
        file.extend_from_slice(&0u16.to_le_bytes());
        file.extend_from_slice(&4u16.to_le_bytes());
        file.extend_from_slice(&(sub as u32).to_le_bytes());
        file.extend_from_slice(&(dprp.len() as u32).to_le_bytes());
        file.extend_from_slice(dprp);
        let at = 8 + GAME_LUMP * 16;
        file[at..at + 4].copy_from_slice(&(game_at as u32).to_le_bytes());
        let len = (file.len() - game_at) as u32;
        file[at + 4..at + 8].copy_from_slice(&len.to_le_bytes());
        file
    }

    #[test]
    fn reads_the_detail_lump() {
        let file = bsp_with(&lump_bytes(&[
            (TYPE_MODEL, 0, 0),
            (TYPE_SPRITE, ORIENT_SCREEN_VERTICAL, 0),
        ]));
        let lump = read(&file).expect("detail lump");
        assert_eq!(lump.models, ["models/props/bush.mdl"]);
        assert_eq!(lump.sprites.len(), 1);
        assert_eq!(lump.sprites[0].ul, Vec2::new(-6.0, 12.0));
        assert_eq!(lump.objects.len(), 2);
        let o = lump.objects[1];
        assert_eq!(o.origin, Vec3::new(10.0, 20.0, 30.0));
        assert_eq!(o.angles.y, 90.0);
        assert_eq!(
            (o.kind, o.orientation, o.sway, o.shape_angle, o.shape_size),
            (1, 2, 127, 30, 25)
        );
        assert_eq!(o.scale, 2.0);
        assert!((o.light[0] - 128.0 * 2.0 / 255.0).abs() < 1e-6);
        assert_eq!(lump.objects[0].kind, TYPE_MODEL);
    }
}
