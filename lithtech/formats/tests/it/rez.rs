//! The `.rez` reader against specs/combat_arms/rez_archive.md on archives
//! written in code (`fixture`), no install needed.

use std::io::ErrorKind;

use lithtech_formats::rez::{RezArchive, RezCipher, RezHeader, RezKeys, decode_type, encode_type, open_dir};
use source_formats::fs::FileSource;

use crate::fixture::{Dir, build, put_number};

fn open(bytes: Vec<u8>) -> std::io::Result<RezArchive> {
    RezArchive::from_bytes("test.rez", bytes, &RezKeys::new())
}

/// "Type code packing" examples.
#[test]
fn type_codes_match_the_spec_examples() {
    assert_eq!(encode_type("XML").to_le_bytes(), [0x4C, 0x4D, 0x58, 0x00]);
    assert_eq!(encode_type("dat").to_le_bytes(), [0x54, 0x41, 0x44, 0x00]);
    assert_eq!(encode_type("DTX").to_le_bytes(), [0x58, 0x54, 0x44, 0x00]);
    assert_eq!(decode_type(0x00584D4C), "XML");
    assert_eq!(decode_type(encode_type("LTB")), "LTB");
    assert_eq!(decode_type(0), "");
}

/// Test cases 2 to 5: the check block of XML.rez's header, rebuilt from
/// the spec's bytes around fields that sum to its check number.
#[test]
fn header_check_block_from_the_spec() {
    let mut h = build("V1_RESOURCES_KEY", Dir::new("XML"))[..0xEC].to_vec();
    // Test case 6's fields (root offset, size, time, next write).
    for (at, v) in [(0xC7, 8_378_563u32), (0xCB, 20), (0xCF, 0), (0xD3, 8_348_081)] {
        h[at..at + 4].copy_from_slice(&v.to_le_bytes());
    }
    h[0x80..0xA0].fill(0xEE);
    h[0x80..0x89].copy_from_slice(b"26508971\0");
    // Test case 4's leftover bytes after the terminator.
    h[0x89..0xA0].copy_from_slice(&[
        0x32, 0x7D, 0x77, 0x2E, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x3C, 0x1F, 0xE4, 0x00, 0x11, 0x00, 0x00,
        0x00, 0x69, 0x32, 0x38, 0x79,
    ]);
    h[0xA2..0xC2].fill(0xEE);
    put_number(&mut h, 0xA2, 16_726_664);
    assert_eq!((h[0x7F], h[0xA1], h[0xA0], h[0xC2]), (0x01, 0x10, 0x10, 0x01));
    let header = RezHeader::parse(&h, 8_378_583, "XML.rez").unwrap();
    assert_eq!(header.check, 16_726_664);
    assert!(header.check_matches_fields());
    assert_eq!(header.title, "V1_RESOURCES_KEY");
    assert_eq!(header.key_scheme(), Some("V1"));

    // Rule 7: the root block must lie inside the file.
    assert!(RezHeader::parse(&h, 8_378_582, "XML.rez").is_err());
}

/// Test case 8: negative check numbers parse as signed 32-bit.
#[test]
fn negative_check_numbers() {
    let mut h = build("V2_RESOURCES_KEY", Dir::new("WORLDS"))[..0xEC].to_vec();
    for (at, v) in [(0xC7, 1_094_698_128u32), (0xCB, 23), (0xCF, 0), (0xD3, 1_094_695_292)] {
        h[at..at + 4].copy_from_slice(&v.to_le_bytes());
    }
    h[0x80..0x8C].copy_from_slice(b"-2095826432\0");
    h[0xA2..0xAE].copy_from_slice(b"-2105573853\0");
    let header = RezHeader::parse(&h, 1_094_698_151, "WORLDS.rez").unwrap();
    assert_eq!(header.check as i32, -2_105_573_853);
    assert!(header.check_matches_fields());
    assert_eq!(header.key_scheme(), Some("V2"));
}

#[test]
fn reads_a_plain_archive() {
    let top = Dir::new("TEST")
        .file("hello", "txt", b"hello, rez".to_vec())
        .file("map", "dat", vec![7u8; 1000])
        .file("empty", "xml", vec![])
        .file("noext", "", b"x".to_vec())
        .dir(
            Dir::new("SUB")
                .dir(Dir::new("DEEPER").file("a", "ltb", b"deep".to_vec()))
                .dir(Dir::new("EMPTY")),
        );
    let rez = open(build("LithTech Resource File", top)).unwrap();
    assert!(rez.header().check_matches_fields());
    assert_eq!(rez.header().key_scheme(), None);
    assert!(!rez.locked());
    assert_eq!(rez.read("test/hello.txt").unwrap().unwrap(), b"hello, rez");
    assert_eq!(rez.read("test/map.dat").unwrap().unwrap(), vec![7u8; 1000]);
    assert_eq!(rez.read("test/empty.xml").unwrap().unwrap(), Vec::<u8>::new());
    assert_eq!(rez.read("test/noext").unwrap().unwrap(), b"x");
    assert_eq!(rez.read("test/sub/deeper/a.ltb").unwrap().unwrap(), b"deep");
    assert!(rez.read("test/missing.txt").is_none());
    let mut paths: Vec<_> = rez.entries().into_iter().map(|e| e.path).collect();
    paths.sort();
    assert_eq!(
        paths,
        [
            "test/empty.xml",
            "test/hello.txt",
            "test/map.dat",
            "test/noext",
            "test/sub/deeper/a.ltb"
        ]
    );
}

#[test]
fn an_empty_archive_is_valid() {
    let mut bytes = build("LithTech Resource File", Dir::new("X"));
    bytes[0xCB..0xCF].copy_from_slice(&0u32.to_le_bytes());
    // The check pair still matches each other; S no longer matches the
    // fields, which the reader does not enforce.
    let rez = open(bytes).unwrap();
    assert!(rez.entries().is_empty());
}

/// Every reader rule rejects what breaks it, with `InvalidData`.
#[test]
fn rejects_broken_headers_and_directories() {
    let good = build(
        "LithTech Resource File",
        Dir::new("TEST").file("a", "txt", b"x".to_vec()),
    );
    assert!(open(good.clone()).is_ok());
    let broken = |what: &str, f: &dyn Fn(&mut Vec<u8>)| {
        let mut b = good.clone();
        f(&mut b);
        let err = open(b).err().unwrap_or_else(|| panic!("{what}: accepted"));
        assert_eq!(err.kind(), ErrorKind::InvalidData, "{what}: {err}");
    };
    broken("short file", &|b| b.truncate(0xEB));
    broken("check number", &|b| b[0x80] = if b[0x80] == b'1' { b'2' } else { b'1' });
    broken("check head byte", &|b| b[0x7F] ^= 1);
    broken("check tail byte", &|b| b[0xC2] ^= 1);
    broken("marker", &|b| b[0x3E] = 0x00);
    broken("version", &|b| b[0xC3] = 2);
    broken("root past the end", &|b| {
        let len = b.len() as u32;
        b[0xC7..0xCB].copy_from_slice(&len.to_le_bytes());
    });
    broken("unknown record kind", &|b| {
        let root = u32::from_le_bytes(b[0xC7..0xCB].try_into().unwrap()) as usize;
        b[root] = 7;
    });
    broken("sub-block past the end", &|b| {
        let root = u32::from_le_bytes(b[0xC7..0xCB].try_into().unwrap()) as usize;
        b[root + 4..root + 8].copy_from_slice(&u32::MAX.to_le_bytes());
    });

    // Stock marker values are accepted byte by byte (rule 5).
    let mut stock = good.clone();
    stock[0x00] = 0x0D;
    stock[0x7E] = 0x1A;
    assert!(open(stock).is_ok());
    // The other version value (rule 6).
    let mut v = good.clone();
    v[0xC3..0xC7].copy_from_slice(&150_784u32.to_le_bytes());
    assert!(open(v).is_ok());
}

/// A keyed archive reads only with its scheme's key; the error tells where
/// keys go.
#[test]
fn keyed_archives_need_their_scheme_key() {
    let cipher = RezCipher::aes256(&[1; 32], &[2; 16]);
    let plain = b"<?xml version=\"1.0\"?><root>payload for the test</root>".to_vec();
    let mut payload = plain.clone();
    cipher.encrypt(&mut payload);
    let bytes = build("V2_RESOURCES_KEY", Dir::new("TEST").file("doc", "xml", payload));

    let locked = RezArchive::from_bytes("keyed.rez", bytes.clone(), &RezKeys::new()).unwrap();
    assert!(locked.locked());
    let err = locked.read("test/doc.xml").unwrap().unwrap_err();
    assert_eq!(err.kind(), ErrorKind::PermissionDenied);
    assert!(err.to_string().contains("[games.combat_arms.keys.V2]"), "{err}");

    let wrong = RezKeys::from([("V1".to_string(), cipher.clone())]);
    assert!(
        RezArchive::from_bytes("keyed.rez", bytes.clone(), &wrong)
            .unwrap()
            .locked(),
        "a V1 key must not unlock V2"
    );

    let keys = RezKeys::from([("V2".to_string(), cipher)]);
    let rez = RezArchive::from_bytes("keyed.rez", bytes, &keys).unwrap();
    assert_eq!(rez.read("test/doc.xml").unwrap().unwrap(), plain);
    assert_eq!(
        rez.read_raw("test/doc.xml").unwrap().unwrap()[48..],
        plain[48..],
        "tail stored plain"
    );
}

/// "Cipher by title": each family reads back through the archive with a
/// made-up key: V1 Twofish-CBC, V2 AES-256-CBC (the first `size & !15`
/// bytes encrypted, the `size & 15` tail stored plain), `LithTech Resource
/// File` raw. A key of the other family's cipher does not decrypt.
#[test]
fn each_title_family_reads_back() {
    let key = [0x5A; 32];
    let iv = [0xC3; 16];
    // 16 * 4 + 7: whole blocks and a 7-byte plain tail; plus a short file.
    let plain: Vec<u8> = (0..71u8).map(|i| b'a' + i % 26).collect();
    for (title, scheme, cipher, other) in [
        ("V1_RESOURCES_KEY", "V1", RezCipher::twofish(&key, &iv), RezCipher::aes256(&key, &iv)),
        ("V2_RESOURCES_KEY", "V2", RezCipher::aes256(&key, &iv), RezCipher::twofish(&key, &iv)),
    ] {
        let mut stored = plain.clone();
        cipher.encrypt(&mut stored);
        assert_eq!(stored[64..], plain[64..], "{title}: tail stored plain");
        assert_ne!(stored[..64], plain[..64]);
        let bytes = build(
            title,
            Dir::new("TEST")
                .file("doc", "dat", stored)
                .file("tiny", "txt", b"short".to_vec()),
        );
        let keys = RezKeys::from([(scheme.to_string(), cipher)]);
        let rez = RezArchive::from_bytes("family.rez", bytes.clone(), &keys).unwrap();
        assert!(!rez.locked());
        assert_eq!(rez.read("test/doc.dat").unwrap().unwrap(), plain, "{title}");
        assert_eq!(rez.read("test/tiny.txt").unwrap().unwrap(), b"short", "{title}");

        let wrong = RezKeys::from([(scheme.to_string(), other)]);
        let rez = RezArchive::from_bytes("family.rez", bytes, &wrong).unwrap();
        let read = rez.read("test/doc.dat").unwrap().unwrap();
        assert_ne!(read[..64], plain[..64], "{title}: the other cipher must not decrypt");
        assert_eq!(read[64..], plain[64..]);
    }
    let raw = build("LithTech Resource File", Dir::new("TEST").file("doc", "dat", plain.clone()));
    let keys = RezKeys::from([
        ("V1".to_string(), RezCipher::twofish(&key, &iv)),
        ("V2".to_string(), RezCipher::aes256(&key, &iv)),
    ]);
    let rez = RezArchive::from_bytes("raw.rez", raw, &keys).unwrap();
    assert_eq!(rez.header().key_scheme(), None);
    assert_eq!(rez.read("test/doc.dat").unwrap().unwrap(), plain, "raw family reads as stored");
}

/// "Names": within one archive, the first of two equal names wins; paths
/// are case-insensitive.
#[test]
fn first_name_wins_and_lookups_ignore_case() {
    let top = Dir::new("TEST")
        .file("same", "txt", b"first".to_vec())
        .file("SAME", "TXT", b"second".to_vec());
    let rez = open(build("LithTech Resource File", top)).unwrap();
    assert_eq!(rez.entries().len(), 1);
    let mut mount = source_formats::fs::Mount::default();
    mount.push(rez);
    assert_eq!(mount.read("TEST\\Same.TXT").unwrap(), b"first");
}

/// `open_dir`: a folder's archives in name order, locked ones named.
#[test]
fn open_dir_mounts_a_folder() {
    let dir = std::env::temp_dir().join(format!("lithtech-rez-test-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let a = build(
        "LithTech Resource File",
        Dir::new("A").file("x", "txt", b"from a".to_vec()),
    );
    let b = build("V1_RESOURCES_KEY", Dir::new("B").file("y", "xml", b"secret!".to_vec()));
    std::fs::write(dir.join("A.rez"), a).unwrap();
    std::fs::write(dir.join("B.REZ"), b).unwrap();
    std::fs::write(dir.join("notes.txt"), "not an archive").unwrap();
    let (mount, locked) = open_dir(&dir, &RezKeys::new()).unwrap();
    assert_eq!(mount.layers().count(), 2);
    assert_eq!(locked, ["B.REZ"]);
    assert_eq!(mount.read("a/x.txt").unwrap(), b"from a");
    assert_eq!(mount.read("b/y.xml").unwrap_err().kind(), ErrorKind::PermissionDenied);
    std::fs::remove_dir_all(&dir).unwrap();
}
