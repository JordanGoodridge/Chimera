# Combat Arms: rez archive

Source basis: reference material the user pointed the spec session to on the game's resource archives (reading, writing and the payload cipher), checked against the Steam install on the Windows PC (see Test cases for the build caveat). The cipher scheme it describes is an older one than the Steam build's; see Encryption. The per-title cipher families (V1 Twofish, V2 AES-256, plain) are format facts the user supplied on 2026-10-19 (no key bytes), confirmed on the Linux dev box's Steam install with the user's keys (Test case 10).
Status: draft

Sections: Summary; Units and conventions; Header; Directory; File data; Encryption (V1 and V2 separately); Compression; Per-archive and per-file variations; Edge cases; Test cases; Open questions.

---

## Summary

- A Combat Arms `.rez` is a stock LithTech "RezMgr Version 1" archive with two changes:
  1. Different marker bytes replace the CR/LF/EOF bytes in the text preamble.
  2. A 68-byte (0x44) integrity-check block is inserted after the preamble. This pushes all numeric header fields 0x44 bytes later. The header is 236 bytes (0xEC), where stock is 168 (0xA8).
- **Directory:** the same as stock. It is plaintext, made of nested blocks of variable-length records, and sits at the end of the file after all file data.
- **Encryption, in the scheme the reference material describes:**
  - Only file payloads were ever encrypted. The header and directory are always plaintext.
  - The cipher is Twofish with a 256-bit key in CBC mode, with a fixed 128-bit IV that restarts at the beginning of every file.
  - Only whole 16-byte blocks are encrypted; the last `size mod 16` bytes stay plaintext. There is no padding, and stored size equals plaintext size.
  - One key/IV pair serves every encrypted archive.
- **V1 / V2 in the Steam build:** the user title selects the cipher family.
  - `LithTech Resource File`: payloads stored raw.
  - `V1_RESOURCES_KEY`: Twofish-256-CBC, as in the reference scheme but with its own key.
  - `V2_RESOURCES_KEY`: **AES-256-CBC** (not Twofish), with its own 32-byte key and 16-byte IV.
  - In both keyed families only the first `size & ~15` bytes are encrypted; the last `size & 15` bytes are plaintext.
  - The reference scheme ignores the title and has a single key, which decrypts neither family in the Steam build.
- **Compression:** none in the reference scheme.

## Units and conventions

- All integers are unsigned 32-bit little-endian ("u32") unless stated otherwise. Header numeric fields are **not** 4-byte aligned (they start at 0xC3).
- Offsets are absolute byte offsets from the start of the archive file. Archives are limited to 4 GiB by the u32 offsets.
- "cstring" means bytes up to and including a single 0x00 terminator, with no length prefix. Names are 8-bit ASCII in practice.
- Timestamps are u32 seconds since 1970-01-01 UTC (Unix time).
- "Type code" means a file extension of up to 4 characters, packed into a u32 (see Directory).
- Hex dumps list bytes in file order.

## Header

The header is a fixed 236 bytes (0xEC) at offset 0. File data starts right after it, at 0xEC.

| Offset | Size | Type | Meaning |
|---|---|---|---|
| 0x00 | 1 | byte | Marker A1. Combat Arms: 0x26. Stock: 0x0D. |
| 0x01 | 1 | byte | Marker A2. Combat Arms: 0x23. Stock: 0x0A. |
| 0x02 | 60 | text | File-type text. The writer fills it with spaces, then writes `RezMgr Version 1 Copyright (C) 1995 MONOLITH INC.` with no terminator. Not validated by the reader. |
| 0x3E | 1 | byte | Marker B1. Combat Arms: 0x21. Stock: 0x0D. |
| 0x3F | 1 | byte | Marker B2. Combat Arms: 0x22. Stock: 0x0A. |
| 0x40 | 60 | text | User title, padded with spaces and not NUL-terminated. Readers strip trailing spaces. Observed values: `LithTech Resource File`, `V1_RESOURCES_KEY`, `V2_RESOURCES_KEY`. |
| 0x7C | 1 | byte | Marker C1. Combat Arms: 0x25. Stock: 0x0D. |
| 0x7D | 1 | byte | Marker C2. Combat Arms: 0x27. Stock: 0x0A. |
| 0x7E | 1 | byte | End marker. Combat Arms: 0x2A. Stock: 0x1A. |
| 0x7F | 1 | byte | Check-head byte, stored obfuscated. Written as 0x01. |
| 0x80 | 32 | cstring area | Obfuscated check number. ASCII signed decimal and a 0x00, followed by **undefined leftover bytes** up to 0x9F. |
| 0xA0 | 1 | byte | Check-tail byte, stored obfuscated. Written as 0x10. |
| 0xA1 | 1 | byte | Check-head byte, plain. Written as 0x10. |
| 0xA2 | 32 | cstring area | Plain check number. ASCII signed decimal and a 0x00, followed by undefined leftover bytes up to 0xC1. |
| 0xC2 | 1 | byte | Check-tail byte, plain. Written as 0x01. |
| 0xC3 | 4 | u32 | Format version. The writer always writes 1. The reader accepts 1 or 150784 (0x00024D00). |
| 0xC7 | 4 | u32 | Root directory block offset. |
| 0xCB | 4 | u32 | Root directory block size in bytes. |
| 0xCF | 4 | u32 | Root directory timestamp. 0 in the observed archives. |
| 0xD3 | 4 | u32 | "Next write position" at pack time: the end of file data, which is also where the directory area starts. |
| 0xD7 | 4 | u32 | Archive last-modified timestamp. |
| 0xDB | 4 | u32 | Largest key-array count (a hint only). |
| 0xDF | 4 | u32 | Largest directory-name size including the NUL (a hint only). |
| 0xE3 | 4 | u32 | Largest file-name size including the NUL (a hint only). |
| 0xE7 | 4 | u32 | Largest description size including the NUL (a hint only). |
| 0xEB | 1 | byte | "Sorted" flag. 1 means file data is grouped contiguously per directory. |

**Stock LithTech RezMgr v1 for comparison:** the first 0x7F bytes have the same shape, using the stock marker values above. The numeric fields 0xC3..0xEB of the table above sit at **0x7F..0xA7** with the same order and meaning, and the header is 168 bytes long. Stock has no check block. This comparison is inferred, not checked against a stock archive (Open questions).

### Header check block (integrity obfuscation, not encryption)

**Writer:**
1. Compute `S = (RootDirOffset + RootDirSize + RootDirTime + NextWritePos) mod 2^32`.
2. At 0xA2, write S as a **signed** 32-bit decimal string.
3. At 0x80, write `S XOR 23807011` (0x016B4423) as a signed 32-bit decimal string.
4. Write the check-head byte plain as 0x10 at 0xA1, and obfuscated as `0x10 XOR 0x11 = 0x01` at 0x7F.
5. Write the check-tail byte plain as 0x01 at 0xC2, and obfuscated as `0x01 XOR 0x11 = 0x10` at 0xA0.

The header is not zero-filled before it is written. That is why bytes after each string's 0x00 in the two 32-byte areas are leftover memory.

**Reader (main archive)** rejects the file unless all of the following hold:
1. The file is at least 236 bytes long.
2. `byte[0x7F] XOR 0x11 == byte[0xA1]`.
3. The decimal number at 0x80, parsed as a signed 32-bit value, is XORed with 23807011. That result, formatted as signed decimal, equals the string at 0xA2. The comparison ignores case, which makes no difference for digits.
4. `byte[0xA0] XOR 0x11 == byte[0xC2]`.
5. Each marker byte is either its Combat Arms value or its stock value, checked independently per byte.
6. The version is 1 or 150784.
7. The root block lies inside the file: `RootDirOffset <= filesize` and `RootDirSize <= filesize - RootDirOffset`.

A root size of 0 means an empty archive, which is valid. The game's reader does **not** check that S matches the actual header fields; our reader may choose to.

**Reader (additional overlay archive opened on top of a main one):** every marker byte must be the Combat Arms value; stock values are not accepted here. See Edge cases for a version-check quirk.

**Consequence:** a true stock 168-byte LithTech header fails the check, so this reader cannot open stock archives. The check fields live where stock keeps its numbers.

## Directory

The directory is a tree of **blocks**. The header points at the root block. Each block is a flat run of records with no count and no terminator; the reader consumes records until it has read exactly `block size` bytes. Every record begins with a u32 **kind**.

### Directory record (kind = 1)

| Offset in record | Size | Type | Meaning |
|---|---|---|---|
| 0 | 4 | u32 | Kind = 1 |
| 4 | 4 | u32 | Absolute offset of this subdirectory's block |
| 8 | 4 | u32 | Size of that block in bytes. 0 means an empty directory, which is not read. |
| 12 | 4 | u32 | Directory timestamp |
| 16 | var | cstring | Directory name, a single path component |

### File record (kind = 0)

| Offset in record | Size | Type | Meaning |
|---|---|---|---|
| 0 | 4 | u32 | Kind = 0 |
| 4 | 4 | u32 | Absolute offset of the file data |
| 8 | 4 | u32 | Data size in bytes. This is both the stored and the logical size. |
| 12 | 4 | u32 | File timestamp |
| 16 | 4 | u32 | Numeric ID. The Combat Arms writer always writes 0; stock tools could write real IDs. |
| 20 | 4 | u32 | Type code (the packed extension) |
| 24 | 4 | u32 | Key count K. The Combat Arms writer always writes 0. |
| 28 | var | cstring | File name **without** extension |
| … | var | cstring | Description. The Combat Arms writer always writes an empty string (a single 0x00). An empty description means "none". |
| … | 4·K | u32[K] | Key values. Skipped by the reader. |

**Record size:**
- A directory record is `16 + len(name) + 1` bytes.
- A file record written by Combat Arms is `28 + len(name) + 1 + 1` bytes.

**Type code packing.** An extension of n characters (n ≤ 4) is converted to upper case. Character i goes into byte `n-1-i` of the u32, counting bytes from least significant. Unused high bytes are 0. In file order:
- `XML` is stored as `4C 4D 58 00` (u32 0x00584D4C).
- `DAT` is stored as `54 41 44 00`.
- `DTX` is stored as `58 54 44 00`.

To decode, take the highest non-zero byte as the first character and read down to byte 0. A type code of 0 means no extension. The packer cannot store extensions longer than 4 characters: for those it appends the extension to the name and uses type 0.

**Full path of a file:** the directory names from the root downwards, the file name, then `.` and the decoded type, joined with `\`. The root block itself has an empty name.

**Nesting:**
- Blocks can nest to any depth.
- In the observed archives, the root block holds exactly one directory record named after the archive stem (`XML`, `WORLDS`, `TEXTURES`), and all content sits under it. The packing pipeline relies on this.
- The writer emits each block's directory records first, then file records grouped by type. Readers must not rely on that order.
- Directory blocks are written depth-first: children first, the root block last, at the end of the file.

**Names:**
- File and directory names are upper-case by convention.
- Lookups are case-insensitive unless a "lower case allowed" mode is enabled.
- When a name is repeated within one directory and type, the first one read wins in the main archive. An overlay archive can be told to replace it.

## File data

- A file's bytes are `[offset, offset + size)`, taken directly from the file record. There is no per-file header, alignment or padding.
- Data normally starts at 0xEC. The directory area starts at the header's "next write position".
- The sorted flag only means each directory's files are contiguous. The order of files inside the data area need not match directory record order, and in the observed archives it does not.
- The archive layer returns stored bytes as they are. Any decryption was done by the code that consumed each file (text, XML, string-list, layout and attribute loaders), on the whole file buffer, starting at payload offset 0.

## Encryption (V1 and V2 separately)

### What is encrypted

| Part | Encrypted? |
|---|---|
| Header numbers | No. Plain little-endian. The check block is XOR obfuscation using the public constants above. |
| Directory blocks | No. Plaintext in every archive inspected, including V1, V2 and plain archives. |
| File payloads | Only in some archives. Payloads are the only thing ever encrypted. |

### Reference scheme (older than the Steam build)

- **Algorithm:** Twofish with a 128-bit block, 16 rounds, and a 256-bit key.
- **Key format:**
  - The key is a 32-byte (256-bit) key, written as 64 hexadecimal ASCII characters that decode in string order to the 32 key bytes. This is the standard byte order: the published Twofish test vectors apply unchanged. No key is given here.
  - The IV is 16 bytes, stored as 32 hexadecimal ASCII characters and decoded the same way.
  - If either string is missing or too short, the game's cipher setup silently falls back to an all-`0`-character key or IV. In other words the key or IV becomes all zero bytes.
- **Mode:** standard CBC. The IV is reset to the fixed IV at the start of **each file**, so two files that begin with the same plaintext begin with the same ciphertext.
- **Coverage:** let `N = floor(size / 16) * 16`.
  - Bytes `[0, N)` are CBC-encrypted.
  - The final `size − N` bytes (0 to 15) are stored **unencrypted**.
  - A file shorter than 16 bytes is stored entirely in the clear.
  - There is no padding and no length field, so ciphertext length equals plaintext length.
- **Decryption steps for one file:**
  1. Read `size` bytes.
  2. Set `prev = IV`.
  3. For each 16-byte block C at offsets 0, 16, …, N−16: `P = TwofishDecrypt(key, C) XOR prev`, then `prev = C`.
  4. Copy the tail bytes unchanged.
- **Inputs that do not vary:** nothing per archive or per file goes into the cipher. There is no file ID, offset, name, size or title input. One key/IV pair served every archive that used encryption.
- **Which archives:**
  - The packer of that scheme encrypted every payload it wrote.
  - The client decrypted only files read through its text-resource loaders, associated with the ATTRIBUTES, GMS and XML archives.
  - Other archives, such as TEXTURES, are plaintext and were read raw.

### Cipher by title (Steam build)

The archive's user title (header 0x40, trailing spaces stripped) selects the family. Nothing else in the archive records it.

| Title | Payloads | Key |
|---|---|---|
| `LithTech Resource File` | Stored raw (unencrypted) | none |
| `V1_RESOURCES_KEY` | Twofish-256-CBC | V1: 32-byte key, 16-byte IV |
| `V2_RESOURCES_KEY` | AES-256-CBC | V2: 32-byte key, 16-byte IV |

- For V1 and V2 alike, per file: the IV restarts at the fixed IV, bytes `[0, size & ~15)` are CBC-decrypted, and the remaining `size & 15` tail bytes are plaintext, copied unchanged. A file under 16 bytes is entirely plaintext. No padding; stored size equals plaintext size. These are the "Decryption steps for one file" above, with AES as the block cipher for V2.
- Keys are supplied by the user at run time and never appear in this spec or the repository.
- A reader should treat an unknown title as raw (every title observed is one of the three).

### V1 (title `V1_RESOURCES_KEY`, e.g. XML.rez)

- **The reference scheme has no V1 handling.** Nothing reads the title to choose a key. The Steam build uses Twofish-CBC with its own key (Cipher by title).
- Observed on the Steam archive (statistics only, without decrypting):
  - The payloads have about 7.9 to 8.0 bits/byte of entropy.
  - In the files sampled, the trailing `size mod 16` bytes are printable text. This matches "whole blocks encrypted, tail plain".
  - 300 of 306 files share an identical first 64 bytes of ciphertext. This matches a deterministic fixed-IV chained mode and a key that does not depend on the file.
  - No 16-byte block repeats inside the largest file, which suggests chaining rather than ECB.
- Decrypting with the reference scheme's key and IV produces high-entropy output. In CBC, a wrong IV alone corrupts only the first block, so **the key differs**, if the cipher and mode are the same.
- **Confirmed:** the same scheme with a different 32-byte key and IV, selected by the title. Decrypted XML files start with a UTF-8 byte-order mark (`EF BB BF`) and then `<?xml version`.

### V2 (title `V2_RESOURCES_KEY`, e.g. WORLDS.rez)

- **The reference scheme has no V2 handling.**
- Observed before the cipher was known: the payloads are high entropy (about 7.95 bits/byte), and the reference scheme's key does not decrypt them. No two of the 36 files share a first ciphertext block, and the tails are not printable; both are expected of binary content.
- **Resolved:** V2 differs from V1 by algorithm as well as key: AES-256-CBC, same mode, same fixed per-file IV restart, same plaintext tail rule. There is no extra layer: decrypted payloads are the plain file formats (Test case 10).

### Plain archives (title `LithTech Resource File`, e.g. TEXTURES.rez)

Payloads are plaintext. Entropy is low, and about 34k of the 87k aligned 16-byte blocks in the largest file are repeats. The directory is plaintext.

## Compression

None. The format has no compression flag, codec, or separate "packed size" field. For V1, the plaintext text tails show the payloads are not compressed. V2 payloads decrypt directly to the plain file formats, so they are not compressed either.

## Per-archive and per-file variations

- **Per archive:**
  - The user title. In the Steam build it selects the cipher family (Cipher by title); in the reference scheme it selects nothing.
  - Marker bytes, Combat Arms or stock, checked byte by byte.
  - Version 1 or 150784.
  - The check-number strings, which are derived from header fields.
  - Timestamps and the sorted flag.
  - Whether payloads are encrypted, and with which cipher and key: only the title says so.
- **Per file:**
  - Offset, size, timestamp, type and name.
  - In the reference scheme, nothing about the encryption varies per file except where the plaintext tail starts (it depends on the file size). The IV is reset at the start of each file.
  - ID and key count are always 0 in Combat Arms-written archives.

## Edge cases

- **Leftover bytes:** bytes after the 0x00 in 0x80..0x9F and 0xA2..0xC1 are uninitialized memory. Ignore them, and never require them to be zero.
- **Negative check numbers:** check numbers can be negative, e.g. `-2095826432`. Parse them as signed 32-bit and reinterpret as unsigned before XOR.
- **Zero-filled gaps in the directory area.** The writer advances its write position twice per directory block. As a result, every directory block **except the last one written (the root)** is followed by zero bytes equal in length to that block. Locate blocks only by offset, never by assuming they are contiguous. Example: in XML.rez, the root block comes after a 15,241-byte gap of zeros.
- **Empty archives:** the root size is 0. Empty subdirectories have block size 0 and are not read.
- **Zero-length files:** the writer stores no data for them, and their offset may be 0. Do not seek or validate the offset when size is 0.
- **Unknown record kinds:** the game's reader treats any kind other than 1 as a file record. A robust reader should reject the block.
- **Bounds checks:** the game's reader bounds-checks only the root block against the file size. Subdirectory blocks, file ranges and cstrings that run past the end of a block are not checked. Our reader should check all of them.
- **Overlay version check:** when an additional archive is opened over a main one, the game's reader checks the **main** archive's version value, not the overlay's. Treat that as a bug. A reader should validate each archive's own version.
- **Version 150784:** this value is accepted only for the main archive. Its meaning is unknown; it may belong to the engine's own resource archive.
- **Title width:** the title is exactly 60 bytes. A longer title would be truncated by the writer.
- **Writer only:** re-saving an item that was already written re-encrypts the in-memory buffer in place, so double encryption is possible. Readers do not need to handle this.
- **Zero-padded output from some consumers:** a few consumers used a streaming decryptor that emits the final partial chunk raw and zero-padded to 16 bytes, or an extra 16 zero bytes when the size is a multiple of 16. Its output can therefore be up to 16 bytes longer than the file. An archive reader should return exactly `size` bytes.

## Test cases

All values below come from the observed header bytes. Values marked "(verified locally)" were also read from the same Steam files on this PC, as numbers and structure only.

1. **Marker bytes (XML.rez, and the same in WORLDS.rez and TEXTURES.rez):**
   - 0x00–0x01 = `26 23`: Combat Arms A1/A2, where stock has `0D 0A`.
   - 0x3E–0x3F = `21 22`: Combat Arms B1/B2.
   - 0x7C–0x7E = `25 27 2A`: Combat Arms C1/C2 and the end marker.

   These marker bytes, together with the always-plaintext directory, are why TEXTURES.rez also "looks odd". The directory is plain, but it sits at the end of the file (offset given at 0xC7), not near the start.
2. **Check head:** 0x7F = `01`. Then `0x01 XOR 0x11 = 0x10`, which must equal the byte at 0xA1. Observed 0xA1 = `10` (verified locally).
3. **Check number:**
   - 0x80 holds `32 36 35 30 38 39 37 31 00`, which is the string `"26508971"` followed by 0x00.
   - `26508971 XOR 23807011 = 16726664` (in hex: `0x01947EAB XOR 0x016B4423 = 0x00FF3A88`).
   - So 0xA2 must hold `"16726664"` followed by 0x00 (verified locally).
4. **Leftover bytes:** 0x89..0x9F, i.e. `32 7D 77 2E 00 00 00 00 00 00 00 3C 1F E4 00 11 00 00 00 69 32 38 79`, follow the terminator and have no meaning. They differ between archives (verified locally).
5. **Check tail (verified locally):** 0xA0 = `10`. Then `0x10 XOR 0x11 = 0x01`, which must equal the byte at 0xC2 (`01`).
6. **Numeric fields of XML.rez (verified locally; file size 8,378,583):**

   | Field | Value |
   |---|---|
   | version | 1 |
   | root offset | 8,378,563 (0x007FD8C3) |
   | root size | 20 |
   | root time | 0 |
   | next write | 8,348,081 (0x007F61B1) |
   | archive time | 1,785,117,922 |
   | largest key array | 0 |
   | largest directory name | 4 |
   | largest file name | 41 |
   | largest description | 0 |
   | sorted | 1 |

   Checks:
   - `8378563 + 20 + 0 + 8348081 = 16726664`, which equals the plain check number.
   - Root offset + root size = 8,378,583, which is the file size.
7. **Root block of XML.rez (20 bytes, verified locally):** `01 00 00 00 | B1 61 7F 00 | 89 3B 00 00 | E2 BC 66 6A | 58 4D 4C 00`. This is a directory record: kind 1, block at 8,348,081, size 15,241, timestamp 1,785,117,922, name `XML`. Size check: 16 + 3 + 1 = 20.
8. **WORLDS.rez, a signed example (verified locally):**
   - 0x80 = `"-2095826432"` and 0xA2 = `"-2105573853"`.
   - Fields: root offset 1,094,698,128, root size 23, root time 0, next write 1,094,695,292.
   - Sum = 2,189,393,443 = 0x827F7E23, which is −2,105,573,853 as a signed value. XOR 23,807,011 gives 0x83143A00 = 2,199,140,864, which is −2,095,826,432 as a signed value.
   - The root block is a single directory record named `WORLDS`, 16 + 6 + 1 = 23 bytes.
9. **Known plaintext without a key:**
   - (a) Every directory block, in all three archives.
   - (b) In XML.rez, the type code bytes `4C 4D 58 00` appear in every file record.
   - (c) In V1 archives, the last `size mod 16` bytes of each payload are plaintext.
   - (d) In V1, files sharing a plaintext prefix share ciphertext block by block. 300 of the 306 XML files share their first four ciphertext blocks. A decryptor that is correct must produce identical first 64 plaintext bytes for all of them.
   - (e) The first payload starts at 0xEC in all three archives.
10. **Header checks after decryption (with the user's keys).** A correct key and cipher make each type's payload start with its own header. Our tests use these as the check that a family's cipher and key are right:

    | Type | First bytes after decryption |
    |---|---|
    | `.dtx` texture | u32 resType 0 at 0x00, i32 version −5 (`FB FF FF FF`) at 0x08 |
    | `.ltb` model | `01 00 09 00` |
    | `.dat` world | `55 00 00 00` (version 85) |
    | `.pcx` | `0A 05 01 08` |
    | `.bmp` | `42 4D` |
    | `.png` | the PNG signature `89 50 4E 47 0D 0A 1A 0A` |

    - (a) V2 (AES-256): every V2 archive that holds textures (24 of the 36 V2 archives on the dev box) has its first texture by path start with the DTX header and parse whole. The first file of each type above in each V2 archive starts with that type's header (dev box: 24 textures, 12 models, 3 worlds, 1 each of PCX, BMP, PNG).
    - (b) V2: the first map of WORLDS.rez and of WORLDS4.rez parse as version-85 worlds; all 72 maps of WORLDS, WORLDS4 and WORLDS5 do.
    - (c) V2: `chars_m_body/cm_body_alphateam_ki.ltb` (8 pieces, 53 nodes) and the M4A1's first-person `guns_m_pv_ar/pv_ar_m4a1_sh.ltb` (1 piece, 50 nodes) parse as models.
    - (d) V1 (Twofish): ATTRIBUTES.rez's `attributes/weapons.txt` (9,007,972 bytes) parses as an attribute file (4,737 sections, 2,321 `Weapon` records). XML.rez's files start with a byte-order mark and `<?xml version`, which also shows the V1 IV is right (a wrong IV corrupts the first block).
    - (e) Decrypting with the other family's cipher, or with a wrong key, gives none of these headers; the `size & 15` tail is identical either way.

## Open questions

1. **Key selection.** Resolved: the title is the selector (Cipher by title). The keys are the user's and are not recorded here.
2. **V2 scheme.** Resolved: AES-256-CBC with its own key and IV, the same per-file IV restart and plaintext tail as V1, no compression or extra layer (Test case 10).
3. **V1 IV.** The user's V1 IV decrypts the first block correctly (Test case 10(d)); whether it equals the reference scheme's IV was not compared and does not matter to a reader.
4. **Steam build mismatch.** The Steam install on this PC reports build id 25456765 in its manifest, not 25595979. Its XML.rez header bytes 0x00–0x9F still match the bytes in the brief exactly.
5. **Version 150784 (0x00024D00).** The meaning of this alternative version value is unknown.
6. **Stock layout.** The stock layout is inferred from the stock marker values the reader still accepts. It has not been checked against an unmodified stock archive.
