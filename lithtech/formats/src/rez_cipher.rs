//! The `.rez` payload cipher, per specs/combat_arms/rez_archive.md,
//! "Encryption (V1 and V2 separately)": Twofish with a 256-bit key in CBC
//! mode, the IV fixed and restarted at the start of every file; only whole
//! 16-byte blocks are encrypted, the last `size mod 16` bytes stay plain,
//! and there is no padding (stored size is plaintext size).
//!
//! Keys come from the user's local config at run time (`[games.combat_arms.keys.<scheme>]`
//! in chimera.local.toml) and never appear in the repo, in errors or in
//! `Debug` output.
//!
//! Known shortcuts: which key a title selects (`V1_RESOURCES_KEY`,
//! `V2_RESOURCES_KEY`), and whether V2 uses this scheme at all, are the
//! spec's open questions 1 to 3; we assume the title selects a key of the
//! same scheme until a key confirms it.

use std::collections::HashMap;

use twofish::{
    Twofish,
    cipher::{Block, BlockCipherDecrypt, BlockCipherEncrypt, KeyInit},
};

/// One key and IV: decrypts (and, for tests and tools, encrypts) a file's
/// payload.
#[derive(Clone)]
pub struct RezCipher {
    twofish: Twofish,
    iv: [u8; 16],
}

impl std::fmt::Debug for RezCipher {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("RezCipher { .. }")
    }
}

/// Ciphers by key scheme: `V1`, `V2` (the title's prefix, `rez::RezHeader::key_scheme`).
pub type RezKeys = HashMap<String, RezCipher>;

impl RezCipher {
    /// "Key format": a 32-byte key and a 16-byte IV, in standard byte
    /// order (the published Twofish test vectors apply unchanged).
    pub fn new(key: &[u8; 32], iv: &[u8; 16]) -> Self {
        Self {
            twofish: Twofish::new_from_slice(key).expect("a 32-byte Twofish key"),
            iv: *iv,
        }
    }

    /// From hexadecimal text (64 characters of key, 32 of IV), as the
    /// local config holds them. Errors name `what`, never the text.
    pub fn from_hex(key: &str, iv: &str, what: &str) -> Result<Self, String> {
        Ok(Self::new(
            &hex::<32>(key, &format!("{what} key"))?,
            &hex::<16>(iv, &format!("{what} iv"))?,
        ))
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
            self.twofish.decrypt_block(&mut block);
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
            self.twofish.encrypt_block(&mut block);
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
    fn byte_order_matches_the_reference_vector() {
        let cipher = RezCipher::new(&[0; 32], &[0; 16]);
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

    /// "Coverage": whole blocks chained, the tail and short files plain.
    #[test]
    fn tail_stays_plain_and_round_trips() {
        let cipher = RezCipher::new(&[3; 32], &[9; 16]);
        let plain: Vec<u8> = (0..53u8).collect();
        let mut data = plain.clone();
        cipher.encrypt(&mut data);
        assert_ne!(data[..48], plain[..48]);
        assert_eq!(data[48..], plain[48..], "the size mod 16 tail stays plaintext");
        cipher.decrypt(&mut data);
        assert_eq!(data, plain);

        let mut short = b"short".to_vec();
        cipher.encrypt(&mut short);
        assert_eq!(short, b"short", "files under 16 bytes are stored in the clear");
    }

    /// The IV restarts per file, so equal plaintext prefixes give equal
    /// ciphertext prefixes (the spec's test case 9(d)); blocks chain, so
    /// a repeated block inside one file does not repeat.
    #[test]
    fn fixed_iv_per_file_and_chaining() {
        let cipher = RezCipher::new(&[5; 32], &[6; 16]);
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

    #[test]
    fn hex_keys_parse_and_errors_hide_the_text() {
        let key = "00".repeat(32);
        let iv = "00".repeat(16);
        assert!(RezCipher::from_hex(&key, &iv, "combat_arms V1").is_ok());
        let err = RezCipher::from_hex("abcdef", &iv, "combat_arms V1").unwrap_err();
        assert!(err.contains("combat_arms V1 key") && !err.contains("abcdef"), "{err}");
        let err = RezCipher::from_hex(&key, &"zz".repeat(16), "x").unwrap_err();
        assert!(!err.contains("zz"), "{err}");
        assert_eq!(format!("{:?}", RezCipher::new(&[1; 32], &[2; 16])), "RezCipher { .. }");
    }
}
