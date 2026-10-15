//! The recording player's input in a client (POV) demo: a `dem_usercmd`
//! frame (type 5) per command the client sent, holding the command's
//! outgoing sequence number and the command itself, delta-encoded against
//! an empty command (Orange Box layout, from the public descriptions of
//! the format; no game code). SourceTV demos have none.
//!
//! Layout, each field behind a "sent" bit (absent: the base's value):
//! `command_number` u32, `tick_count` u32, view angles pitch/yaw/roll as
//! f32 each, forward/side/up move as f32 each, `buttons` u32, `impulse`
//! u8, `weaponselect` 11 bits (an entity index) then, behind its own bit,
//! `weaponsubtype` 6 bits, mouse dx and dy as i16 each.
//!
//! The random seed is not sent: both sides make it from the command
//! number (`random_seed`, specs/cs_source/weapons.md 4.2).
//!
//! Known shortcuts:
//! - Commands are decoded against an empty one. A frame without a command
//!   number (a stream encoded against the previous command instead) is
//!   decoded again against the previous command (`DemoReader`); no client
//!   demo has been seen yet to say which one CS:S writes.

use super::bits::{BitReader, BitWriter, Result};

/// Bits of `weaponselect` (an entity index, `MAX_EDICT_BITS`).
pub const WEAPON_SELECT_BITS: u32 = 11;
/// Bits of `weaponsubtype`.
pub const WEAPON_SUBTYPE_BITS: u32 = 6;

/// Source's command buttons (`IN_*`).
pub mod buttons {
    pub const ATTACK: u32 = 1 << 0;
    pub const JUMP: u32 = 1 << 1;
    pub const DUCK: u32 = 1 << 2;
    pub const FORWARD: u32 = 1 << 3;
    pub const BACK: u32 = 1 << 4;
    pub const USE: u32 = 1 << 5;
    pub const MOVELEFT: u32 = 1 << 9;
    pub const MOVERIGHT: u32 = 1 << 10;
    pub const ATTACK2: u32 = 1 << 11;
    pub const RELOAD: u32 = 1 << 13;
    /// The walk key (+speed).
    pub const SPEED: u32 = 1 << 17;
}

/// One user command.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct UserCmd {
    pub command_number: u32,
    /// The client's tick when it made the command.
    pub tick_count: u32,
    /// Pitch, yaw, roll (degrees).
    pub view_angles: [f32; 3],
    pub forward_move: f32,
    pub side_move: f32,
    pub up_move: f32,
    pub buttons: u32,
    pub impulse: u8,
    /// The weapon entity to switch to (0: none).
    pub weapon_select: u32,
    pub weapon_subtype: u32,
    pub mouse_dx: i16,
    pub mouse_dy: i16,
}

impl UserCmd {
    /// Read one command delta-encoded against `from`. Also says whether
    /// the command number was sent.
    pub fn read(r: &mut BitReader, from: &UserCmd) -> Result<(UserCmd, bool)> {
        let mut c = *from;
        let numbered = r.bit()?;
        if numbered {
            c.command_number = r.u32()?;
        }
        if r.bit()? {
            c.tick_count = r.u32()?;
        }
        for a in &mut c.view_angles {
            if r.bit()? {
                *a = r.f32()?;
            }
        }
        for m in [&mut c.forward_move, &mut c.side_move, &mut c.up_move] {
            if r.bit()? {
                *m = r.f32()?;
            }
        }
        if r.bit()? {
            c.buttons = r.u32()?;
        }
        if r.bit()? {
            c.impulse = r.u8()?;
        }
        if r.bit()? {
            c.weapon_select = r.bits(WEAPON_SELECT_BITS)?;
            if r.bit()? {
                c.weapon_subtype = r.bits(WEAPON_SUBTYPE_BITS)?;
            }
        }
        if r.bit()? {
            c.mouse_dx = r.i16()?;
        }
        if r.bit()? {
            c.mouse_dy = r.i16()?;
        }
        Ok((c, numbered))
    }

    /// Write this command delta-encoded against `from` (fields equal to
    /// `from`'s are left out), as `read` reads it.
    pub fn write(&self, w: &mut BitWriter, from: &UserCmd) {
        let mut field = |sent: bool, f: &mut dyn FnMut(&mut BitWriter)| {
            w.bit(sent);
            if sent {
                f(w);
            }
        };
        field(self.command_number != from.command_number, &mut |w| {
            w.u32(self.command_number)
        });
        field(self.tick_count != from.tick_count, &mut |w| w.u32(self.tick_count));
        for i in 0..3 {
            let a = self.view_angles[i];
            field(a.to_bits() != from.view_angles[i].to_bits(), &mut |w| w.f32(a));
        }
        for (m, f) in [
            (self.forward_move, from.forward_move),
            (self.side_move, from.side_move),
            (self.up_move, from.up_move),
        ] {
            field(m.to_bits() != f.to_bits(), &mut |w| w.f32(m));
        }
        field(self.buttons != from.buttons, &mut |w| w.u32(self.buttons));
        field(self.impulse != from.impulse, &mut |w| w.u8(self.impulse));
        let subtype = self.weapon_subtype != from.weapon_subtype;
        field(self.weapon_select != from.weapon_select, &mut |w| {
            w.bits(self.weapon_select, WEAPON_SELECT_BITS);
            w.bit(subtype);
            if subtype {
                w.bits(self.weapon_subtype, WEAPON_SUBTYPE_BITS);
            }
        });
        field(self.mouse_dx != from.mouse_dx, &mut |w| w.u16(self.mouse_dx as u16));
        field(self.mouse_dy != from.mouse_dy, &mut |w| w.u16(self.mouse_dy as u16));
    }

    /// The command's shared random seed (`MD5(n as 4 LE bytes)`, the
    /// 32-bit LE integer at digest bytes 6-9, `& 0x7FFFFFFF`; spec
    /// weapons.md 4.2).
    pub fn random_seed(&self) -> u32 {
        let d = md5_4(self.command_number.to_le_bytes());
        u32::from_le_bytes([d[6], d[7], d[8], d[9]]) & 0x7FFF_FFFF
    }
}

/// MD5 (RFC 1321) of a 4-byte message: one padded block.
fn md5_4(msg: [u8; 4]) -> [u8; 16] {
    const S: [u32; 64] = [
        7, 12, 17, 22, 7, 12, 17, 22, 7, 12, 17, 22, 7, 12, 17, 22, 5, 9, 14, 20, 5, 9, 14, 20, 5, 9, 14, 20, 5, 9, 14,
        20, 4, 11, 16, 23, 4, 11, 16, 23, 4, 11, 16, 23, 4, 11, 16, 23, 6, 10, 15, 21, 6, 10, 15, 21, 6, 10, 15, 21, 6,
        10, 15, 21,
    ];
    let k: Vec<u32> = (0..64)
        .map(|i| ((i as f64 + 1.0).sin().abs() * 4_294_967_296.0) as u32)
        .collect();
    let mut block = [0u8; 64];
    block[..4].copy_from_slice(&msg);
    block[4] = 0x80;
    block[56..64].copy_from_slice(&(32u64).to_le_bytes());
    let m: Vec<u32> = block
        .chunks(4)
        .map(|c| u32::from_le_bytes([c[0], c[1], c[2], c[3]]))
        .collect();
    let (a0, b0, c0, d0) = (0x6745_2301u32, 0xefcd_ab89u32, 0x98ba_dcfeu32, 0x1032_5476u32);
    let (mut a, mut b, mut c, mut d) = (a0, b0, c0, d0);
    for i in 0..64 {
        let (f, g) = match i / 16 {
            0 => ((b & c) | (!b & d), i),
            1 => ((d & b) | (!d & c), (5 * i + 1) % 16),
            2 => (b ^ c ^ d, (3 * i + 5) % 16),
            _ => (c ^ (b | !d), (7 * i) % 16),
        };
        let f = f.wrapping_add(a).wrapping_add(k[i]).wrapping_add(m[g]);
        a = d;
        d = c;
        c = b;
        b = b.wrapping_add(f.rotate_left(S[i]));
    }
    let mut out = [0u8; 16];
    for (i, v) in [
        a0.wrapping_add(a),
        b0.wrapping_add(b),
        c0.wrapping_add(c),
        d0.wrapping_add(d),
    ]
    .iter()
    .enumerate()
    {
        out[i * 4..i * 4 + 4].copy_from_slice(&v.to_le_bytes());
    }
    out
}

#[cfg(test)]
mod tests {
    use super::super::bits::tests::pack;
    use super::*;

    #[test]
    fn a_command_reads_field_by_field() {
        // Command 1234 at tick 99, looking 10 down and 90 left, forward
        // 400, jump held, weapon entity 300 picked; everything else unsent.
        let f = |v: f32| v.to_bits();
        let data = pack(&[
            (1, 1),
            (1234, 32),
            (1, 1),
            (99, 32),
            (1, 1),
            (f(10.0), 32),
            (1, 1),
            (f(90.0), 32),
            (0, 1),
            (1, 1),
            (f(400.0), 32),
            (0, 1),
            (0, 1),
            (1, 1),
            (buttons::JUMP | buttons::FORWARD, 32),
            (0, 1),
            (1, 1),
            (300, 11),
            (0, 1),
            (1, 1),
            (0xFFFE, 16),
            (0, 1),
        ]);
        let (c, numbered) = UserCmd::read(&mut BitReader::new(&data), &UserCmd::default()).unwrap();
        assert!(numbered);
        assert_eq!(
            c,
            UserCmd {
                command_number: 1234,
                tick_count: 99,
                view_angles: [10.0, 90.0, 0.0],
                forward_move: 400.0,
                buttons: buttons::JUMP | buttons::FORWARD,
                weapon_select: 300,
                mouse_dx: -2,
                ..Default::default()
            }
        );
    }

    #[test]
    fn unsent_fields_keep_the_base() {
        let base = UserCmd {
            command_number: 7,
            side_move: -250.0,
            buttons: buttons::DUCK,
            ..Default::default()
        };
        // Only the command number sent.
        let data = pack(&[(1, 1), (8, 32), (0, 12)]);
        let (c, _) = UserCmd::read(&mut BitReader::new(&data), &base).unwrap();
        assert_eq!(c.command_number, 8);
        assert_eq!((c.side_move, c.buttons), (-250.0, buttons::DUCK));
    }

    #[test]
    fn writing_then_reading_gives_the_command_back() {
        let cmds = [
            UserCmd::default(),
            UserCmd {
                command_number: 0xDEAD_BEEF,
                tick_count: 5,
                view_angles: [-89.0, 359.5, 0.25],
                forward_move: -450.0,
                side_move: 450.0,
                up_move: 320.0,
                buttons: buttons::ATTACK | buttons::ATTACK2 | buttons::RELOAD | buttons::SPEED,
                impulse: 100,
                weapon_select: 2047,
                weapon_subtype: 63,
                mouse_dx: i16::MIN,
                mouse_dy: i16::MAX,
            },
        ];
        for from in &cmds {
            for c in &cmds {
                let mut w = BitWriter::new();
                c.write(&mut w, from);
                let data = w.finish();
                let (back, _) = UserCmd::read(&mut BitReader::new(&data), from).unwrap();
                assert_eq!(back, *c);
            }
        }
    }

    #[test]
    fn seeds_come_from_the_command_number() {
        // The spec's test cases (weapons.md T1, T1b).
        let seed = |n| {
            UserCmd {
                command_number: n,
                ..Default::default()
            }
            .random_seed()
        };
        assert_eq!(seed(1), 1_997_239_609);
        assert_eq!(seed(1) & 255, 57);
        assert_eq!(seed(2), 1_318_994_272);
        assert_eq!(seed(100), 236_147_958);
        assert_eq!(seed(12345), 36_020_103);
    }
}
