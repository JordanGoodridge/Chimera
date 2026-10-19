//! Twofish with a 256-bit key, from the published algorithm (Schneier et
//! al., "Twofish: A 128-Bit Block Cipher"), with the key-dependent S-boxes
//! and MDS multiply folded into four 256-entry tables at key setup. The
//! RustCrypto `twofish` crate recomputes them for every block, which
//! decrypts V1 archives at about 2 MB/s; with the tables it is a few
//! hundred MB/s. Checked against the published vectors and, in tests,
//! against that crate.

/// The 4-bit permutation tables of q0 and q1.
const Q0_T: [[u8; 16]; 4] = [
    [8, 1, 7, 13, 6, 15, 3, 2, 0, 11, 5, 9, 14, 12, 10, 4],
    [14, 12, 11, 8, 1, 2, 3, 5, 15, 4, 10, 6, 7, 0, 9, 13],
    [11, 10, 5, 14, 6, 13, 9, 0, 12, 8, 15, 3, 2, 4, 7, 1],
    [13, 7, 15, 4, 1, 2, 6, 14, 9, 11, 3, 0, 8, 5, 12, 10],
];
const Q1_T: [[u8; 16]; 4] = [
    [2, 8, 11, 13, 15, 7, 6, 14, 3, 1, 9, 4, 0, 10, 12, 5],
    [1, 14, 2, 11, 4, 12, 3, 7, 6, 13, 10, 5, 15, 9, 0, 8],
    [4, 12, 7, 5, 1, 6, 9, 10, 0, 14, 13, 8, 2, 11, 3, 15],
    [11, 9, 5, 1, 12, 3, 13, 14, 6, 4, 7, 15, 2, 0, 8, 10],
];
const MDS: [[u8; 4]; 4] = [
    [0x01, 0xEF, 0x5B, 0x5B],
    [0x5B, 0xEF, 0xEF, 0x01],
    [0xEF, 0x5B, 0x01, 0xEF],
    [0xEF, 0x01, 0xEF, 0x5B],
];
const RS: [[u8; 8]; 4] = [
    [0x01, 0xA4, 0x55, 0x87, 0x5A, 0x58, 0xDB, 0x9E],
    [0xA4, 0x56, 0x82, 0xF3, 0x1E, 0xC6, 0x68, 0xE5],
    [0x02, 0xA1, 0xFC, 0xC1, 0x47, 0xAE, 0x3D, 0x19],
    [0xA4, 0x55, 0x87, 0x5A, 0x58, 0xDB, 0x9E, 0x03],
];
const MDS_POLY: u16 = 0x169;
const RS_POLY: u16 = 0x14D;

fn q(t: &[[u8; 16]; 4], x: u8) -> u8 {
    let ror4 = |v: u8| ((v >> 1) | (v << 3)) & 15;
    let (a0, b0) = (x >> 4, x & 15);
    let (a1, b1) = (a0 ^ b0, a0 ^ ror4(b0) ^ ((a0 << 3) & 15));
    let (a2, b2) = (t[0][a1 as usize], t[1][b1 as usize]);
    let (a3, b3) = (a2 ^ b2, a2 ^ ror4(b2) ^ ((a2 << 3) & 15));
    let (a4, b4) = (t[2][a3 as usize], t[3][b3 as usize]);
    (b4 << 4) | a4
}

/// Multiplication in GF(2^8) modulo `poly`.
fn gf_mul(mut a: u8, mut b: u8, poly: u16) -> u8 {
    let mut out = 0u8;
    while b != 0 {
        if b & 1 != 0 {
            out ^= a;
        }
        let carry = a & 0x80 != 0;
        a <<= 1;
        if carry {
            a ^= (poly & 0xFF) as u8;
        }
        b >>= 1;
    }
    out
}

/// The key-dependent byte chain of `h` for byte position `j`, with the
/// list `l` (four words, `l[0]` innermost-last as in the paper).
fn h_byte(j: usize, x: u8, l: &[[u8; 4]; 4], q0: &[u8; 256], q1: &[u8; 256]) -> u8 {
    // Which permutation each stage applies to byte `j` (true: q1).
    const STAGES: [[bool; 4]; 5] = [
        [true, false, false, true],
        [true, true, false, false],
        [false, true, false, true],
        [false, false, true, true],
        [true, false, true, false],
    ];
    let q = |stage: usize, v: u8| if STAGES[stage][j] { q1[v as usize] } else { q0[v as usize] };
    // k = 4: the two outer stages, then the common three.
    let mut y = q(0, x) ^ l[3][j];
    y = q(1, y) ^ l[2][j];
    y = q(2, y) ^ l[1][j];
    y = q(3, y) ^ l[0][j];
    q(4, y)
}

fn mds_column(j: usize, y: u8) -> u32 {
    (0..4).fold(0, |acc, i| acc | (gf_mul(y, MDS[i][j], MDS_POLY) as u32) << (8 * i))
}

/// `h(X, L)`: the full function, for the subkeys.
fn h(x: u32, l: &[[u8; 4]; 4], q0: &[u8; 256], q1: &[u8; 256]) -> u32 {
    let b = x.to_le_bytes();
    (0..4).fold(0, |acc, j| acc ^ mds_column(j, h_byte(j, b[j], l, q0, q1)))
}

/// One 256-bit key, expanded.
#[derive(Clone)]
pub(super) struct Twofish256 {
    k: [u32; 40],
    /// `g` per input byte position: `g(X) = t[0][x0] ^ t[1][x1] ^ ...`.
    t: Box<[[u32; 256]; 4]>,
}

impl Twofish256 {
    pub(super) fn new(key: &[u8; 32]) -> Self {
        let q0: [u8; 256] = std::array::from_fn(|x| q(&Q0_T, x as u8));
        let q1: [u8; 256] = std::array::from_fn(|x| q(&Q1_T, x as u8));
        let word = |i: usize| -> [u8; 4] { key[4 * i..4 * i + 4].try_into().unwrap() };
        let even: [[u8; 4]; 4] = std::array::from_fn(|i| word(2 * i));
        let odd: [[u8; 4]; 4] = std::array::from_fn(|i| word(2 * i + 1));
        // S_i = RS . key bytes 8i..8i+8; g uses them in reverse order.
        let s_vec = |i: usize| -> [u8; 4] {
            std::array::from_fn(|r| (0..8).fold(0, |acc, c| acc ^ gf_mul(key[8 * i + c], RS[r][c], RS_POLY)))
        };
        let s: [[u8; 4]; 4] = std::array::from_fn(|i| s_vec(3 - i));
        let mut k = [0u32; 40];
        for i in 0..20u32 {
            let a = h(2 * i * 0x0101_0101, &even, &q0, &q1);
            let b = h((2 * i + 1) * 0x0101_0101, &odd, &q0, &q1).rotate_left(8);
            k[2 * i as usize] = a.wrapping_add(b);
            k[2 * i as usize + 1] = a.wrapping_add(b.wrapping_mul(2)).rotate_left(9);
        }
        let mut t = Box::new([[0u32; 256]; 4]);
        for (j, table) in t.iter_mut().enumerate() {
            for (x, e) in table.iter_mut().enumerate() {
                *e = mds_column(j, h_byte(j, x as u8, &s, &q0, &q1));
            }
        }
        Self { k, t }
    }

    #[inline(always)]
    fn g(&self, x: u32) -> u32 {
        let b = x.to_le_bytes();
        self.t[0][b[0] as usize] ^ self.t[1][b[1] as usize] ^ self.t[2][b[2] as usize] ^ self.t[3][b[3] as usize]
    }

    fn words(block: &[u8; 16]) -> [u32; 4] {
        std::array::from_fn(|i| u32::from_le_bytes(block[4 * i..4 * i + 4].try_into().unwrap()))
    }

    fn store(block: &mut [u8; 16], w: [u32; 4]) {
        for (i, v) in w.iter().enumerate() {
            block[4 * i..4 * i + 4].copy_from_slice(&v.to_le_bytes());
        }
    }

    pub(super) fn encrypt_block(&self, block: &mut [u8; 16]) {
        let k = &self.k;
        let p = Self::words(block);
        let mut r = [p[0] ^ k[0], p[1] ^ k[1], p[2] ^ k[2], p[3] ^ k[3]];
        for round in 0..16 {
            let t0 = self.g(r[0]);
            let t1 = self.g(r[1].rotate_left(8));
            let f0 = t0.wrapping_add(t1).wrapping_add(k[2 * round + 8]);
            let f1 = t0.wrapping_add(t1.wrapping_mul(2)).wrapping_add(k[2 * round + 9]);
            let r2 = (r[2] ^ f0).rotate_right(1);
            let r3 = r[3].rotate_left(1) ^ f1;
            r = [r2, r3, r[0], r[1]];
        }
        Self::store(block, [r[2] ^ k[4], r[3] ^ k[5], r[0] ^ k[6], r[1] ^ k[7]]);
    }

    pub(super) fn decrypt_block(&self, block: &mut [u8; 16]) {
        let k = &self.k;
        let c = Self::words(block);
        // The state after the last round, before the output swap.
        let mut r = [c[2] ^ k[6], c[3] ^ k[7], c[0] ^ k[4], c[1] ^ k[5]];
        for round in (0..16).rev() {
            // r is (r2', r3', r0, r1) of that round.
            let t0 = self.g(r[2]);
            let t1 = self.g(r[3].rotate_left(8));
            let f0 = t0.wrapping_add(t1).wrapping_add(k[2 * round + 8]);
            let f1 = t0.wrapping_add(t1.wrapping_mul(2)).wrapping_add(k[2 * round + 9]);
            let r2 = r[0].rotate_left(1) ^ f0;
            let r3 = (r[1] ^ f1).rotate_right(1);
            r = [r[2], r[3], r2, r3];
        }
        Self::store(block, [r[0] ^ k[0], r[1] ^ k[1], r[2] ^ k[2], r[3] ^ k[3]]);
    }
}

#[cfg(test)]
mod tests {
    use twofish::cipher::{Block, BlockCipherDecrypt, BlockCipherEncrypt, KeyInit};

    use super::*;

    /// The published 256-bit vectors: the zero key on the zero block, and
    /// the first entry of the paper's 256-bit test.
    #[test]
    fn published_vectors() {
        let t = Twofish256::new(&[0; 32]);
        let mut b = [0u8; 16];
        t.encrypt_block(&mut b);
        assert_eq!(
            b,
            [0x57, 0xFF, 0x73, 0x9D, 0x4D, 0xC9, 0x2C, 0x1B, 0xD7, 0xFC, 0x01, 0x70, 0x0C, 0xC8, 0x21, 0x6F]
        );
        t.decrypt_block(&mut b);
        assert_eq!(b, [0; 16]);

        let key: [u8; 32] = [
            0x01, 0x23, 0x45, 0x67, 0x89, 0xAB, 0xCD, 0xEF, 0xFE, 0xDC, 0xBA, 0x98, 0x76, 0x54, 0x32, 0x10, 0x00, 0x11,
            0x22, 0x33, 0x44, 0x55, 0x66, 0x77, 0x88, 0x99, 0xAA, 0xBB, 0xCC, 0xDD, 0xEE, 0xFF,
        ];
        let mut b = [0u8; 16];
        Twofish256::new(&key).encrypt_block(&mut b);
        assert_eq!(
            b,
            [0x37, 0x52, 0x7B, 0xE0, 0x05, 0x23, 0x34, 0xB8, 0x9F, 0x0C, 0xFC, 0xCA, 0xE8, 0x7C, 0xFA, 0x20]
        );
    }

    /// The same blocks as the RustCrypto crate, both ways, on assorted
    /// made-up keys.
    #[test]
    fn matches_the_reference_crate() {
        for seed in 0..8u8 {
            let key: [u8; 32] = std::array::from_fn(|i| (i as u8).wrapping_mul(37).wrapping_add(seed.wrapping_mul(91)));
            let ours = Twofish256::new(&key);
            let theirs = twofish::Twofish::new_from_slice(&key).unwrap();
            for n in 0..16u8 {
                let plain: [u8; 16] = std::array::from_fn(|i| (i as u8 ^ n).wrapping_mul(seed | 1));
                let mut a = plain;
                ours.encrypt_block(&mut a);
                let mut b = Block::<twofish::Twofish>::default();
                b.copy_from_slice(&plain);
                theirs.encrypt_block(&mut b);
                assert_eq!(a[..], b[..], "encrypt, seed {seed}");
                theirs.decrypt_block(&mut b);
                ours.decrypt_block(&mut a);
                assert_eq!(a, plain);
                assert_eq!(b[..], plain[..]);
            }
        }
    }
}
