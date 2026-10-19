//! The texture reader (`lithtech_formats::dtx`) on the synthetic fixtures
//! of specs/combat_arms/textures_dtx.md (F1-F8), written here from the
//! spec's tables (no game data).

use lithtech_formats::dtx::{Dtx, Format};

/// The fixtures' 164-byte header with a size, N and format code.
fn header(w: u16, h: u16, n: u16, format: u8) -> Vec<u8> {
    let mut b = vec![0u8; 164];
    b[4..6].copy_from_slice(&w.to_le_bytes());
    b[6..8].copy_from_slice(&h.to_le_bytes());
    b[8..12].copy_from_slice(&(-5i32).to_le_bytes());
    b[0x0C..0x0E].copy_from_slice(&n.to_le_bytes());
    b[0x10] = 0x08;
    b[0x19] = 1;
    b[0x1A] = format;
    b
}

fn fixture(format: u8, pixels: &[u8]) -> Dtx {
    let mut b = header(4, 4, 1, format);
    b.extend_from_slice(pixels);
    Dtx::parse(b).unwrap()
}

fn rows(rgba: &[u8]) -> Vec<Vec<[u8; 4]>> {
    rgba.as_chunks::<4>().0.chunks(4).map(|r| r.to_vec()).collect()
}

#[test]
fn f1_bgra32() {
    let r0 = [0, 0, 255, 255, 0, 255, 0, 255, 255, 0, 0, 255, 255, 255, 255, 255];
    let r1 = [0, 0, 0, 255, 0, 0, 0, 0, 0x80, 0x80, 0x80, 0x80, 0x56, 0x34, 0x12, 0x78];
    let px: Vec<u8> = [r0, r1, r0, r1].concat();
    let t = fixture(3, &px);
    assert_eq!((t.format, t.offsets.clone()), (Format::Bgra32, vec![164, 228]));
    let r = rows(&t.rgba(0));
    assert_eq!(r[0], [[255, 0, 0, 255], [0, 255, 0, 255], [0, 0, 255, 255], [255, 255, 255, 255]]);
    assert_eq!(r[1], [[0, 0, 0, 255], [0, 0, 0, 0], [128, 128, 128, 128], [0x12, 0x34, 0x56, 0x78]]);
    assert_eq!((r[2].clone(), r[3].clone()), (r[0].clone(), r[1].clone()));
}

#[test]
fn f2_dxt1_four_colours() {
    let t = fixture(4, &[0x00, 0xF8, 0x1F, 0x00, 0xE4, 0x00, 0x55, 0x1B]);
    let (red, blue, p2, p3) = ([255, 0, 0, 255], [0, 0, 255, 255], [170, 0, 85, 255], [85, 0, 170, 255]);
    let r = rows(&t.rgba(0));
    assert_eq!(r[0], [red, blue, p2, p3]);
    assert_eq!(r[1], [red; 4]);
    assert_eq!(r[2], [blue; 4]);
    assert_eq!(r[3], [p3, p2, blue, red]);
}

#[test]
fn f3_dxt1_three_colours() {
    let t = fixture(4, &[0x1F, 0x00, 0x00, 0xF8, 0xF4, 0xFF, 0x50, 0x05]);
    let (blue, red, clear) = ([0, 0, 255, 255], [255, 0, 0, 255], [0, 0, 0, 0]);
    let r = rows(&t.rgba(0));
    assert_eq!(r[0], [blue, red, clear, clear]);
    assert_eq!(r[1], [clear; 4]);
    assert_eq!(r[2], [blue, blue, red, red]);
    assert_eq!(r[3], [red, red, blue, blue]);
}

#[test]
fn f4_dxt3() {
    let t = fixture(
        5,
        &[0x50, 0xFA, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0x00, 0x00, 0x00, 0x00, 0x00, 0x39],
    );
    let r = rows(&t.rgba(0));
    let white = |a: u8| [255, 255, 255, a];
    assert_eq!(r[0], [white(0), white(85), white(170), white(255)]);
    assert_eq!((r[1].clone(), r[2].clone()), (vec![white(255); 4], vec![white(255); 4]));
    assert_eq!(r[3], [[0, 0, 0, 255], [170, 170, 170, 255], [85, 85, 85, 255], white(255)]);
}

#[test]
fn f5_dxt5() {
    let t = fixture(
        6,
        &[0x00, 0xFF, 0x88, 0xC6, 0xFA, 0x49, 0x92, 0x24, 0xE0, 0x07, 0, 0, 0, 0, 0, 0],
    );
    let r = rows(&t.rgba(0));
    let alphas: Vec<Vec<u8>> = r.iter().map(|row| row.iter().map(|p| p[3]).collect()).collect();
    assert_eq!(alphas, [vec![0, 255, 51, 102], vec![153, 204, 0, 255], vec![255; 4], vec![255; 4]]);
    assert!(r.iter().flatten().all(|p| p[..3] == [0, 255, 0]));
}

#[test]
fn f6_mip_chain() {
    assert!(Dtx::parse(header(16, 8, 4, 4)).is_err(), "levels under 4 x 4");
    let mut b = header(16, 8, 2, 4);
    b.resize(244, 0);
    assert_eq!(Dtx::parse(b).unwrap().offsets, [164, 228, 244]);
    let mut b = header(16, 8, 4, 3);
    b.resize(844, 0);
    assert_eq!(Dtx::parse(b).unwrap().offsets, [164, 676, 804, 836, 844]);
}

#[test]
fn f7_rejections() {
    let valid = || {
        let mut b = header(4, 4, 1, 4);
        b.resize(172, 0);
        b
    };
    assert!(Dtx::parse(valid()).is_ok());
    let mut b = valid();
    b[8..12].copy_from_slice(&(-4i32).to_le_bytes());
    assert!(Dtx::parse(b).is_err(), "version -4");
    let mut b = valid();
    b[0] = 1;
    assert!(Dtx::parse(b).is_err(), "resource type 1");
    for n in [0u16, 9] {
        let mut b = valid();
        b[0x0C..0x0E].copy_from_slice(&n.to_le_bytes());
        assert!(Dtx::parse(b).is_err(), "N = {n}");
    }
    let mut b = valid();
    b.pop();
    assert!(Dtx::parse(b).is_err(), "truncated");
    assert!(Dtx::parse(Vec::new()).is_err(), "empty");
}

#[test]
fn f8_command_strings() {
    let with = |cmd: &[u8]| {
        let mut b = header(4, 4, 1, 4);
        b[0x24..0x24 + cmd.len()].copy_from_slice(cmd);
        b.resize(172, 0);
        Dtx::parse(b).unwrap()
    };
    assert_eq!(with(b"alpharef 128").alpha_ref(), Some(128));
    assert_eq!(with(b"ALPHAREF 1").alpha_ref(), Some(1));
    assert_eq!(with(b"aipharef 128").alpha_ref(), None);
    assert_eq!(with(b"alpharef 0").alpha_ref(), None);
    assert_eq!(with(b"alpharef 128; detailtex a.dtx").alpha_ref(), Some(128));
    let t = with(b"detailtex a.dtx\0b.dtx");
    assert_eq!((t.command.as_str(), t.alpha_ref()), ("detailtex a.dtx", None));
}

/// A 4 x 4 texture of `group` with `n` levels (BGRA, zeros).
fn grouped(group: u8, n: u16) -> Dtx {
    let mut b = header(16, 16, n, 3);
    b[0x18] = group;
    let len: usize = (0..n).map(|i| 4 * (16usize >> i) * (16usize >> i)).sum();
    b.resize(164 + len, 0);
    Dtx::parse(b).unwrap()
}

/// Test cases 15 and 16: the shipped DetailLevel 2 (no option changes
/// it), groups 0..3 and 49 up; the N − 1 clamp; DetailLevel 0.
#[test]
fn t15_t16_mip_skip_by_group() {
    use lithtech_formats::dtx::{DETAIL_LEVEL, group_offset};
    assert_eq!(DETAIL_LEVEL, 2);
    let k = |g, n, d| grouped(g, n).skipped_levels(d, 0);
    assert_eq!([0, 1, 2, 3, 49, 60].map(|g| k(g, 4, DETAIL_LEVEL)), [0, 1, 1, 2, 0, 0]);
    assert_eq!(k(3, 2, DETAIL_LEVEL), 1, "clamped to N - 1");
    assert_eq!(k(3, 4, 0), 0);
    // The game's table: GroupOffset1..3 = 1, 1, 2 at d = 2 (every preset).
    assert_eq!([1, 2, 3].map(|g| group_offset(g, DETAIL_LEVEL)), [1, 1, 2]);
    assert_eq!([1, 2, 3].map(|g| group_offset(g, 1)), [0, 1, 1]);
    // MipMapOffset counts alone from group 49, still at least 0.
    assert_eq!(grouped(49, 4).skipped_levels(2, 1), 1);
    assert_eq!(grouped(1, 4).skipped_levels(2, -3), 0);
}

/// One DXT block's texels.
fn block(format: u8, bytes: &[u8]) -> Vec<[u8; 4]> {
    fixture(format, bytes).rgba(0).as_chunks::<4>().0.to_vec()
}

/// Test cases 17-20: the reading to keep (the card's: round to nearest,
/// bit-repeating widening, DXT3 always four colours).
#[test]
fn t17_t20_dxt_rounding() {
    // 17: c0 = 0xF800 (red 255), c1 = 0x0800 (red 8): index 2 at texel 0,
    // index 3 at texel 1.
    let t = block(4, &[0x00, 0xF8, 0x00, 0x08, 0b1110, 0, 0, 0]);
    assert_eq!((t[0][0], t[1][0]), (173, 90));
    // 18: red 16 widens to 132.
    let t = block(4, &[0x00, 0x80, 0x00, 0x00, 0, 0, 0, 0]);
    assert_eq!(t[0][0], 132);
    // 19: DXT5 a0 = 255, a1 = 0: A2..A7 at texels 0..5.
    let idx: u64 = (2..8u64).enumerate().map(|(t, k)| k << (3 * t)).sum();
    let mut b = vec![255, 0];
    b.extend_from_slice(&idx.to_le_bytes()[..6]);
    b.extend_from_slice(&[0; 8]);
    let t = block(6, &b);
    assert_eq!(t[..6].iter().map(|p| p[3]).collect::<Vec<_>>(), [219, 182, 146, 109, 73, 36]);
    // 20: DXT3, c0 = 0x0000 <= c1 = 0xFFFF, index 3, alpha 15: the 2/3
    // entry, opaque.
    let t = block(5, &[0xFF; 8].iter().copied().chain([0x00, 0x00, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF]).collect::<Vec<_>>());
    assert_eq!(t[0], [170, 170, 170, 255]);
}

/// Test cases 21 and 22 on synthetic strings: the detail link is the
/// command's second word as written, its path nothing added (no `.dtx`,
/// no link); the base texture's detail scale and rotation.
#[test]
fn t21_t22_links() {
    use lithtech_formats::dtx::{LinkKind, link_path, split_commands};
    let with = |cmd: &[u8], scale: f32| {
        let mut b = header(4, 4, 1, 4);
        b[0x24..0x24 + cmd.len()].copy_from_slice(cmd);
        b[0x1E..0x22].copy_from_slice(&scale.to_le_bytes());
        b.resize(172, 0);
        Dtx::parse(b).unwrap()
    };
    let t = with(b"detailtex textures\\Etc\\256xSprinkle01.dtx", 0.0);
    let link = t.link().unwrap();
    assert_eq!((link.kind, link.names.as_slice()), (LinkKind::DetailTex, &["textures\\Etc\\256xSprinkle01.dtx".to_string()][..]));
    assert_eq!(link_path(&link.names[0]), "textures/etc/256xsprinkle01.dtx");
    assert_eq!((t.detail_scale, t.detail_rotation), (1.0, 0));
    let t = with(b"detailtex textures\\Etc\\256xSprinkle01", 7.0);
    assert_eq!(link_path(&t.link().unwrap().names[0]), "textures/etc/256xsprinkle01", "nothing added");
    assert_eq!(t.detail_scale, 8.0);
    // The first keyword in the engine's order wins, wherever it is; a
    // whole-word match; too few words: no link.
    let t = with(b"envmap e.dtx; DETAILTEX d.dtx", 0.0);
    assert_eq!(t.link().unwrap().kind, LinkKind::DetailTex);
    assert_eq!(with(b"envmapalpha e.dtx", 0.0).link().unwrap().kind, LinkKind::EnvMapAlpha);
    assert_eq!(with(b"detailtex", 0.0).link(), None);
    assert_eq!(with(b"alpharef 128", 0.0).link(), None);
    // Words: quotes and parentheses group, % escapes, control characters
    // end a command.
    assert_eq!(
        split_commands("detailtex \"a b.dtx\";x (c d) e%;f\x01g"),
        [vec!["detailtex", "a b.dtx"], vec!["x", "c d", "e;f"], vec!["g"]]
    );
}
