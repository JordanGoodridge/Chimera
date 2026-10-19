//! The `.rez` payload ciphers, per specs/combat_arms/rez_archive.md,
//! "Encryption (V1 and V2 separately)". The archive's user title picks the
//! family: `V1_RESOURCES_KEY` is Twofish-256, `V2_RESOURCES_KEY` is
//! AES-256, anything else (`LithTech Resource File`) is stored raw. Both
//! keyed families use CBC with a fixed 16-byte IV restarted at the start of
//! every file; only the first `size & !15` bytes are encrypted, the last
//! `size & 15` stay plain, and there is no padding (stored size is
//! plaintext size).
//!
//! Keys come from the user's local config at run time
//! (`[games.combat_arms.keys.V1]` / `.V2`, hex `key` and `iv`, in
//! chimera.local.toml); the scheme name chooses the cipher. Keys never
//! appear in the repo, in errors or in `Debug` output.

use std::collections::HashMap;

use aes::Aes256;
use twofish::{
    Twofish,
    cipher::{Block, BlockCipherDecrypt, BlockCipherEncrypt, KeyInit},
};

/// The block cipher under CBC, chosen by the key scheme.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RezCipherKind {
    /// `V1_RESOURCES_KEY`.
    Twofish,
    /// `V2_RESOURCES_KEY`.
    Aes256,
}

impl RezCipherKind {
    /// The cipher a key scheme (`V1`, `V2`; `rez::RezHeader::key_scheme`)
    /// uses, if it is one we know.
    pub fn for_scheme(scheme: &str) -> Option<Self> {
        match scheme {
            "V1" => Some(Self::Twofish),
            "V2" => Some(Self::Aes256),
            _ => None,
        }
    }
}

#[derive(Clone)]
enum Algo {
    Twofish(Twofish),
    Aes256(Aes256),
}

/// One key and IV of one family: decrypts (and, for tests and tools,
/// encrypts) a file's payload.
#[derive(Clone)]
pub struct RezCipher {
    algo: Algo,
    iv: [u8; 16],
}

/// Never prints key or IV bytes.
impl std::fmt::Debug for RezCipher {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "RezCipher({:?}, ..)", self.kind())
    }
}

/// Ciphers by key scheme: `V1`, `V2` (the title's prefix, `rez::RezHeader::key_scheme`).
pub type RezKeys = HashMap<String, RezCipher>;

impl RezCipher {
    /// "Key format": a 32-byte key and a 16-byte IV, in standard byte
    /// order (the published test vectors apply unchanged).
    pub fn new(kind: RezCipherKind, key: &[u8; 32], iv: &[u8; 16]) -> Self {
        let algo = match kind {
            RezCipherKind::Twofish => Algo::Twofish(Twofish::new_from_slice(key).expect("a 32-byte Twofish key")),
            RezCipherKind::Aes256 => Algo::Aes256(Aes256::new_from_slice(key).expect("a 32-byte AES key")),
        };
        Self { algo, iv: *iv }
    }

    /// V1's family: Twofish-256-CBC.
    pub fn twofish(key: &[u8; 32], iv: &[u8; 16]) -> Self {
        Self::new(RezCipherKind::Twofish, key, iv)
    }

    /// V2's family: AES-256-CBC.
    pub fn aes256(key: &[u8; 32], iv: &[u8; 16]) -> Self {
        Self::new(RezCipherKind::Aes256, key, iv)
    }

    pub fn kind(&self) -> RezCipherKind {
        match self.algo {
            Algo::Twofish(_) => RezCipherKind::Twofish,
            Algo::Aes256(_) => RezCipherKind::Aes256,
        }
    }

    /// From the local config's `[games.combat_arms.keys.<scheme>]`: the
    /// scheme picks the cipher, `key` and `iv` are hexadecimal text (64
    /// and 32 characters). Errors name `what`, never the text.
    pub fn from_hex(scheme: &str, key: &str, iv: &str, what: &str) -> Result<Self, String> {
        let kind = RezCipherKind::for_scheme(scheme)
            .ok_or_else(|| format!("{what}: unknown key scheme {scheme:?} (expected V1 or V2)"))?;
        Ok(Self::new(
            kind,
            &hex::<32>(key, &format!("{what} key"))?,
            &hex::<16>(iv, &format!("{what} iv"))?,
        ))
    }

    fn decrypt_block(&self, block: &mut Block<Twofish>) {
        match &self.algo {
            Algo::Twofish(c) => c.decrypt_block(block),
            Algo::Aes256(c) => c.decrypt_block(block),
        }
    }

    fn encrypt_block(&self, block: &mut Block<Twofish>) {
        match &self.algo {
            Algo::Twofish(c) => c.encrypt_block(block),
            Algo::Aes256(c) => c.encrypt_block(block),
        }
    }

    /// Decrypt a whole file in place ("Decryption steps for one file"):
    /// each whole block is deciphered and XORed with the previous
    /// ciphertext block (the IV for the first); the tail stays as stored.
    pub fn decrypt(&self, data: &mut [u8]) {
        let mut prev = self.iv;
        for chunk in data.as_chunks_mut::<16>().0 {
            let stored = *chunk;
            let mut block = Block::<Twofish>::default();
            block.copy_from_slice(chunk);
            self.decrypt_block(&mut block);
            for (out, (p, c)) in chunk.iter_mut().zip(block.iter().zip(prev)) {
                *out = p ^ c;
            }
            prev = stored;
        }
    }

    /// The inverse of `decrypt`: for fixtures in tests and for tools.
    pub fn encrypt(&self, data: &mut [u8]) {
        let mut prev = self.iv;
        for chunk in data.as_chunks_mut::<16>().0 {
            let mut block = Block::<Twofish>::default();
            for (b, (p, c)) in block.iter_mut().zip(chunk.iter().zip(prev)) {
                *b = p ^ c;
            }
            self.encrypt_block(&mut block);
            chunk.copy_from_slice(&block);
            prev = *chunk;
        }
    }
}

/// Hex text to exactly `N` bytes; the error never includes the text.
fn hex<const N: usize>(text: &str, what: &str) -> Result<[u8; N], String> {
    let text = text.trim();
    if text.len() != N * 2 || !text.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err(format!("{what}: expected {} hexadecimal characters", N * 2));
    }
    let mut out = [0u8; N];
    for (i, byte) in out.iter_mut().enumerate() {
        *byte = u8::from_str_radix(&text[2 * i..2 * i + 2], 16).unwrap();
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The published Twofish 256-bit vector (zero key, zero block): with a
    /// zero IV, CBC's first block is a plain block encryption.
    #[test]
    fn twofish_byte_order_matches_the_reference_vector() {
        let cipher = RezCipher::twofish(&[0; 32], &[0; 16]);
        let mut block = [0u8; 16];
        cipher.encrypt(&mut block);
        assert_eq!(
            block,
            [
                0x57, 0xFF, 0x73, 0x9D, 0x4D, 0xC9, 0x2C, 0x1B, 0xD7, 0xFC, 0x01, 0x70, 0x0C, 0xC8, 0x21, 0x6F
            ]
        );
        cipher.decrypt(&mut block);
        assert_eq!(block, [0u8; 16]);
    }

    /// FIPS-197 appendix C.3 (AES-256, key 00..1f, block 00112233..ff):
    /// with a zero IV, CBC's first block is a plain block encryption.
    #[test]
    fn aes_byte_order_matches_the_reference_vector() {
        let key: [u8; 32] = std::array::from_fn(|i| i as u8);
        let cipher = RezCipher::aes256(&key, &[0; 16]);
        let mut block: [u8; 16] = std::array::from_fn(|i| (i as u8) * 0x11);
        cipher.encrypt(&mut block);
        assert_eq!(
            block,
            [
                0x8E, 0xA2, 0xB7, 0xCA, 0x51, 0x67, 0x45, 0xBF, 0xEA, 0xFC, 0x49, 0x90, 0x4B, 0x49, 0x60, 0x89
            ]
        );
        cipher.decrypt(&mut block);
        assert_eq!(block, std::array::from_fn(|i| (i as u8) * 0x11));
    }

    /// "Coverage": whole blocks chained, the tail and short files plain,
    /// in both families.
    #[test]
    fn tail_stays_plain_and_round_trips() {
        for kind in [RezCipherKind::Twofish, RezCipherKind::Aes256] {
            let cipher = RezCipher::new(kind, &[3; 32], &[9; 16]);
            let plain: Vec<u8> = (0..53u8).collect();
            let mut data = plain.clone();
            cipher.encrypt(&mut data);
            assert_ne!(data[..48], plain[..48]);
            assert_eq!(data[48..], plain[48..], "{kind:?}: the size & 15 tail stays plaintext");
            cipher.decrypt(&mut data);
            assert_eq!(data, plain);

            let mut short = b"short".to_vec();
            cipher.encrypt(&mut short);
            assert_eq!(short, b"short", "{kind:?}: files under 16 bytes are stored in the clear");
        }
        // The two families differ on the same key.
        let mut a = [0u8; 16];
        let mut b = [0u8; 16];
        RezCipher::twofish(&[3; 32], &[9; 16]).encrypt(&mut a);
        RezCipher::aes256(&[3; 32], &[9; 16]).encrypt(&mut b);
        assert_ne!(a, b);
    }

    /// The IV restarts per file, so equal plaintext prefixes give equal
    /// ciphertext prefixes (the spec's test case 9(d)); blocks chain, so
    /// a repeated block inside one file does not repeat.
    #[test]
    fn fixed_iv_per_file_and_chaining() {
        for kind in [RezCipherKind::Twofish, RezCipherKind::Aes256] {
            let cipher = RezCipher::new(kind, &[5; 32], &[6; 16]);
            let mut a = [b"<?xml version=\"1.0\"?>".as_slice(), &[b'a'; 43]].concat();
            let mut b = [b"<?xml version=\"1.0\"?>".as_slice(), &[b'b'; 43]].concat();
            cipher.encrypt(&mut a);
            cipher.encrypt(&mut b);
            assert_eq!(a[..16], b[..16]);
            assert_ne!(a[16..32], b[16..32]);
            let mut same = [7u8; 32];
            cipher.encrypt(&mut same);
            assert_ne!(same[..16], same[16..]);
        }
    }

    #[test]
    fn scheme_picks_the_cipher_and_errors_hide_the_text() {
        let key = "00".repeat(32);
        let iv = "00".repeat(16);
        let v1 = RezCipher::from_hex("V1", &key, &iv, "combat_arms V1").unwrap();
        let v2 = RezCipher::from_hex("V2", &key, &iv, "combat_arms V2").unwrap();
        assert_eq!(v1.kind(), RezCipherKind::Twofish);
        assert_eq!(v2.kind(), RezCipherKind::Aes256);
        let err = RezCipher::from_hex("V3", &key, &iv, "combat_arms V3").unwrap_err();
        assert!(err.contains("unknown key scheme") && !err.contains(&key), "{err}");
        let err = RezCipher::from_hex("V1", "abcdef", &iv, "combat_arms V1").unwrap_err();
        assert!(err.contains("combat_arms V1 key") && !err.contains("abcdef"), "{err}");
        let err = RezCipher::from_hex("V2", &key, &"zz".repeat(16), "x").unwrap_err();
        assert!(!err.contains("zz"), "{err}");
        let secret = RezCipher::aes256(&[0xAB; 32], &[0xCD; 16]);
        let shown = format!("{secret:?}");
        assert_eq!(shown, "RezCipher(Aes256, ..)");
        assert!(!shown.to_lowercase().contains("ab") && !shown.to_lowercase().contains("cd"));
    }
}
