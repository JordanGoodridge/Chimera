# Combat Arms: textures (.dtx)

Source basis: reference material the user pointed the spec session to on the engine's texture loading (the texture file reader and writer, the pixel-format helpers, the Direct3D texture setup and the world and model render paths that read texture properties), plus the game's performance options. All numbers in Test cases and the statistics were measured on the Steam install on the Windows PC (Steam build 25595979), reading each `.dtx` straight out of the unkeyed archives with a throwaway reader written from `rez_archive.md`. A follow-up session added the engine's software pixel-format converters, the linked-texture loader, the game's graphics options screen and performance presets, and the Steam install's player profile.
Status: draft

Sections: Summary; Units and conventions; Header; Format codes and pixel layouts; Mipmap chain; Sections after the pixels (palette, cube map); What the install contains; Load-time behaviour (version check, mip skipping); Render behaviour from the header (alpha test, fullbright, detail textures, environment maps, wrapping); How other files name a texture, and missing textures; Edge cases; Quirks; Test cases; Synthetic fixtures; Open questions.

---

## Summary

- A `.dtx` is a fixed 164-byte header followed directly by the pixel data of each mipmap level, largest first, with no padding, alignment or per-level header. Combat Arms' install uses one file version only, **-5**, and in every readable file the file ends exactly where the last mipmap ends.
- The install uses four pixel formats: 32-bit BGRA (code 3), DXT1 (code 4), DXT3 (code 5) and DXT5 (code 6). There are no palettised, 16-bit, 24-bit, cube-map or bump-map textures, and no file carries a trailing section.
- Render behaviour is set by a few header bits (fullbright, 16-bit preferences) and by a short text **command string** in the header. The command string can turn on alpha testing (`alpharef N`), link a detail texture (`detailtex`), or link an environment map (`envmap`, `envmapalpha`). The header has no wrap or clamp setting. Textures repeat unless the code drawing them says otherwise.
- Other files name a texture by its path from the game folder, with the `.dtx` extension, `\` as separator and case ignored. The first path component is the archive's root directory (for example `TEXTURES\...`). If a name does not resolve, the texture slot is empty. World surfaces then try a fallback name, and that fallback is not in the Steam install.

## Units and conventions

- Integers are little-endian. "u16", "u32" are unsigned, "i32" signed, "f32" an IEEE-754 single.
- Offsets are byte offsets from the start of the `.dtx` file (that is, from the start of the file's bytes as the archive reader returns them).
- Pixel coordinates: x to the right, y downwards. Row 0 is the first row stored and is the top of the image as it appears on screen at texture coordinate v = 0.
- Colours are given as (R, G, B, A) with 8 bits per channel, 0..255.
- "Level i" is mipmap level i. Level 0 is the full-size image.

## Header

The header is 164 bytes (0xA4). Every field is naturally aligned and there is no padding.

| Offset | Size | Type | Meaning |
|---|---|---|---|
| 0x00 | 4 | u32 | Resource type. Must be 0 (texture). |
| 0x04 | 2 | u16 | Width W of level 0, in pixels. |
| 0x06 | 2 | u16 | Height H of level 0, in pixels. |
| 0x08 | 4 | i32 | File version. Must be -5 (bytes `FB FF FF FF`). |
| 0x0C | 2 | u16 | Mipmap count N stored in the file. Valid range 1..8. |
| 0x0E | 2 | u16 | Section count: how many sections follow the pixel data (see "Sections after the pixels"). Trusted only when flag bit 3 is set. 0 in every file of the install. |
| 0x10 | 4 | u32 | Texture flags (table below). |
| 0x14 | 4 | u32 | Surface flags. The texture's surface type number, for example the material that decides impact effects and footstep sounds. The engine does not read it at run time. The world compiler copies it, cut to 16 bits, into each world surface, and the game reads it back from the world. Common observed values: 0, 10, 20, 30, 40, 204. |
| 0x18 | 1 | u8 | Texture group, 0..255. It picks a per-group console offset that drops top mip levels at load time (see Load-time behaviour). |
| 0x19 | 1 | u8 | Mip levels to use. This is how many levels the renderer builds, starting from the first level it keeps. 0 means 8. Values above what is available are clamped. It often differs from N (see What the install contains). |
| 0x1A | 1 | u8 | Format code (next section). 0 also means 32-bit (code 3). |
| 0x1B | 1 | u8 | Extra levels to drop when the renderer cannot use compressed textures and has to expand DXT data. 0 in the install. |
| 0x1C | 1 | u8 | Editor texture-coordinate shift: an editor-only hint that lets an editor treat a large texture like one 2^k times smaller. Ignored at run time. 0 in the install, except one file with 1. |
| 0x1D | 1 | u8 | Texture priority. Not used at run time. 0 in the install. |
| 0x1E | 4 | f32 | Detail-texture scale, stored as scale − 1. A stored 0.0 therefore means scale 1.0. Not aligned to 4 bytes. |
| 0x22 | 2 | i16 | Detail-texture rotation, in whole degrees. Stored as low byte then high byte. |
| 0x24 | 128 | text | Command string: 8-bit text ending at the first 0x00. Bytes after that 0x00 are leftovers and must be ignored. 533 files in the install have non-zero bytes there. If none of the 128 bytes is 0x00, treat the whole field as the string. |

Bytes 0x18..0x23 are a 12-byte block of small fields. Writers zero all of it before setting the fields.

### Texture flags (offset 0x10)

| Bit | Value | Our name | Meaning | In the install |
|---|---|---|---|---|
| 0 | 0x0001 | fullbright | The alpha channel is a fullbright mask: where alpha is high the texture ignores lighting. See Render behaviour. | 340 files |
| 1 | 0x0002 | prefer 16-bit | Use a 16-bit copy even when the renderer runs 32-bit textures. Only matters for uncompressed textures. | 5 files |
| 2 | 0x0004 | (tool-only) | Used inside the tools while a texture is in memory. Never saved. | never |
| 3 | 0x0008 | sections valid | Says the section count at 0x0E can be trusted. Set by every writer since the count was fixed. Without it, a reader must treat the section count as 0. | every readable file |
| 6 | 0x0040 | no system cache | The loader keeps the file's own pixel format in memory and does not convert it. | never |
| 7 | 0x0080 | prefer 4444 | In 16-bit mode, use 4-4-4-4 ARGB. | 196 files |
| 8 | 0x0100 | prefer 5551 | In 16-bit mode, use 5-5-5-1 ARGB. | 121 files |
| 9 | 0x0200 | keep 32-bit copy | Do not convert the in-memory copy to the device format. | never |
| 10 | 0x0400 | cube map | Six faces; see Sections after the pixels. | never |
| 11 | 0x0800 | bump map | 8-bit U and V bump data. | never |
| 12 | 0x1000 | luminance bump map | 8-bit luminance, U and V. | never |
| 13 | 0x2000 | alpha only | The device texture keeps only alpha. | never |

Other bits are unused. The only flag values seen are 0x8 (9,035 files), 0x9 (303), 0x88 (159), 0x108 (119), 0x89 (35), 0x8A (2), and one file each of 0xA, 0xB, 0x109 and 0x10A.

## Format codes and pixel layouts

| Code | Format | Bytes for a w × h level | In the install |
|---|---|---|---|
| 0 | Read as code 3. A palettised value 0 cannot be stored, because 0 decodes as 32-bit. | as code 3 | never written |
| 1 | 8-bit RGB (no layout is defined) | w·h | never |
| 2 | 16-bit | 2·w·h | never |
| 3 | 32-bit BGRA | 4·w·h | 3,237 files |
| 4 | DXT1 (BC1) | w·h / 2 | 4,068 files |
| 5 | DXT3 (BC2) | w·h | 457 files |
| 6 | DXT5 (BC3) | w·h | 1,895 files |
| 7 | 8-bit indices into a 256-colour, 32-bit palette stored in a section | w·h | never |
| 8 | 24-bit | 3·w·h | never |

A reader for Combat Arms needs codes 0/3, 4, 5 and 6. It should reject any other code as unsupported.

### Code 3: 32-bit

Pixels are stored row by row from the top row, left to right, 4 bytes per pixel, with no row padding. The bytes of a pixel are **B, G, R, A** in that order, so the pixel read as a u32 is 0xAARRGGBB. Alpha 255 is opaque.

### Codes 4, 5, 6: block-compressed

These are the standard DirectX block formats. The image is cut into 4×4 blocks. Blocks are stored row by row from the top-left block. Inside a block, texels are numbered t = 4·y + x (row-major from the top-left).

**Colour block** (8 bytes; it is the whole DXT1 block and the second half of a DXT3 or DXT5 block):

| Offset in block | Size | Meaning |
|---|---|---|
| 0 | 2 | Colour c0, u16 in 5-6-5 form: bits 11..15 red, 5..10 green, 0..4 blue |
| 2 | 2 | Colour c1, the same form |
| 4 | 4 | u32 of 2-bit indices; texel t uses bits 2t and 2t+1. Byte 4 therefore covers row 0, with its low two bits for x = 0. |

Each channel is widened to 8 bits by repeating its top bits:

```math
r_8 = (r_5 \ll 3) \;|\; (r_5 \gg 2), \qquad g_8 = (g_6 \ll 2) \;|\; (g_6 \gg 4), \qquad b_8 = (b_5 \ll 3) \;|\; (b_5 \gg 2)
```

The four palette entries P0..P3 are built per channel from C0 and C1, the widened c0 and c1:

```math
\text{if } c_0 > c_1 \text{ (compared as u16) or the block is DXT3/DXT5:}\quad P_0 = C_0,\; P_1 = C_1,\; P_2 = \tfrac{2C_0 + C_1}{3},\; P_3 = \tfrac{C_0 + 2C_1}{3}
```

```math
\text{if } c_0 \le c_1 \text{ in a DXT1 block:}\quad P_0 = C_0,\; P_1 = C_1,\; P_2 = \tfrac{C_0 + C_1}{2},\; P_3 = \text{transparent black } (0,0,0,0)
```

In DXT1, every colour other than P3 in the 3-colour mode has alpha 255. In DXT3 and DXT5 the colour block is always read in 4-colour mode, whatever the order of c0 and c1. The game hands DXT data to the graphics card, so the rounding of the interpolated entries is the card's. A reader should round to nearest. The fixtures below use only values that divide exactly.

**Rounding** (Open question 10). With texture compression on (the default, and every current card supports DXT) the game never decodes DXT itself: the card does, and current cards compute the 1/3 and 2/3 entries to within one step of the exact value, typically rounded. So round to nearest is the reading to keep for what players see. The engine also has its own software decoder, used only when the card cannot take compressed textures (DXT data is then expanded to 16- or 32-bit). It differs in four ways, given here for a byte-exact comparison with that path:

- channels are widened by ⌊c·255/(2^b − 1)⌋, not by repeating bits (for 5-bit 16: 131, not 132);
- P2, P3 and the 3-colour midpoint truncate: ⌊(2C₀ + C₁)/3⌋, ⌊(C₀ + 2C₁)/3⌋, ⌊(C₀ + C₁)/2⌋;
- DXT5 alpha entries truncate: ⌊((8−k)a₀ + (k−1)a₁)/7⌋ and ⌊((6−k)a₀ + (k−1)a₁)/5⌋; DXT3 alpha is a₄·17 (exact either way);
- it picks the 3-colour mode by c0 ≤ c1 in DXT3 and DXT5 blocks too.

**DXT3 alpha** (the first 8 bytes of a 16-byte block): 16 explicit 4-bit alphas. Texel t is in byte ⌊t/2⌋: even t in the low nibble, odd t in the high nibble. The 8-bit alpha is a₄ · 17.

**DXT5 alpha** (the first 8 bytes of a 16-byte block):

| Offset in block | Size | Meaning |
|---|---|---|
| 0 | 1 | Alpha a0 |
| 1 | 1 | Alpha a1 |
| 2 | 6 | A 48-bit little-endian value of 3-bit indices; texel t uses bits 3t..3t+2 |

```math
a_0 > a_1:\quad A_0 = a_0,\; A_1 = a_1,\; A_k = \tfrac{(8-k)\,a_0 + (k-1)\,a_1}{7} \;\; (k = 2..7)
```

```math
a_0 \le a_1:\quad A_0 = a_0,\; A_1 = a_1,\; A_k = \tfrac{(6-k)\,a_0 + (k-1)\,a_1}{5} \;\; (k = 2..5),\; A_6 = 0,\; A_7 = 255
```

## Mipmap chain

Level i has these dimensions. The halving is an integer shift and never rounds up:

```math
w_i = W \gg i, \qquad h_i = H \gg i
```

Its size in bytes, S(f, w, h), is the formula in the table above. For the compressed codes it is exactly w·h/2 or w·h, **not** rounded up to whole blocks. Levels follow each other directly:

```math
\text{offset}_0 = 164, \qquad \text{offset}_{i+1} = \text{offset}_i + S(f, w_i, h_i), \qquad \text{end of pixels} = \text{offset}_N
```

- The tools only write sizes 8, 16, 32, 64, 128, 256, 512 and 1024 on each axis. Width and height need not be equal: 2,294 files in the install are non-square.
- Each level is a box-filtered half of the one before it. That matters only to a writer.
- In the install, no compressed level is smaller than 4×4. Where that holds, the size formula above gives the same answer as counting whole blocks. Some 32-bit textures go down to 1×2 or 2×2 (see Test cases).
- A reader should reject a compressed file whose last level is under 4 pixels on either axis. Such a file never appears in the install, and the engine's formula would give part of a block.

## Sections after the pixels (palette, cube map)

None of the 9,657 readable files in the install has a section. A reader can ignore everything after the pixels. The layout is given so that the reader can skip sections or recognise them.

- **Order.** Sections follow the last pixel byte, one after another. There are as many as the section count at 0x0E, and only when flag bit 3 is set.
- **Section header** (32 bytes; inferred from the in-memory record with natural alignment, never observed in a file):

| Offset in section | Size | Type | Meaning |
|---|---|---|---|
| 0 | 15 | text | Section type, ending at a 0x00 |
| 15 | 10 | text | Section name, ending at a 0x00 (empty in practice) |
| 25 | 3 | — | Padding |
| 28 | 4 | u32 | Data length L, not counting this header |
| 32 | L | bytes | Data |

- **Palette** (format code 7): section type `PALLETE32` (spelled that way in the files), L = 1024, 256 entries of 4 bytes. The runtime loader does not read sections, so a code-7 texture can never get its palette in the game. See Open questions on the channel order.
- **Cube map** (flag bit 10): level data for face +X sits in the normal place. A section of type `CUBEMAPDATA` follows it. Its data holds the faces −X, +Y, −Y, +Z, −Z in that order, each a complete mip chain of the same size as the +X data. The runtime reads +X, skips the 32-byte section header, then reads the five faces in a row.

## What the install contains

Measured on Steam build 25595979:

- **Files.** 22,810 `.dtx` entries in 56 of the 108 archives.
  - 9,658 are in the 32 archives titled `LithTech Resource File` (unkeyed). 9,657 of them are readable, and 1 has length 0 (Edge cases).
  - 13,152 are in 24 archives titled `V2_RESOURCES_KEY`. Their payloads are encrypted (see `rez_archive.md`), so their headers could not be read here. One is empty. The other 13,151 are all at least 164 bytes, and size − 164 is a multiple of 8, which is what the layout above predicts.
- **Version.** Every readable header has resource type 0 and version -5. No other version occurs.
- **Formats.** 32-bit: 3,237; DXT1: 4,068; DXT3: 457; DXT5: 1,895.
- **Mipmap counts.**

| N | 1 | 2 | 3 | 4 | 5 | 6 | 7 | 8 |
|---|---|---|---|---|---|---|---|---|
| Files | 4,196 | 157 | 124 | 3,562 | 395 | 720 | 476 | 27 |

  Most HUD and interface textures and many model skins have 1 level. World textures mostly have 4 to 7.
- **Sizes.** Every readable file is exactly 164 + Σ S(f, wᵢ, hᵢ) bytes long. Nothing follows the pixels.
- **Section count.** 0 everywhere.
- **Texture groups.** 0 for 9,321 files. Groups 1 (111 files), 2 (157), 3 (40) and 5 (28) occur, all in `FX.rez`.
- **Mip levels to use (0x19).** Equal to N in 9,139 files. The other 518: 0 in 459, smaller than N in 41, larger than N in 18, including one 8-level file with 41.
- **Detail and editor fields (0x1B..0x23).** Zero except in three files. One stores detail scale 7.0 (effective 8.0); one stores −1.0 (effective 0); one has editor shift 1.
- **Command strings.** 895 files have one. By first word (case ignored): `alpharef` 840 (values 128 and 1); `envmapalpha` 47; `detailtex` 3; `envmap` 1; the misspelled `aipharef` 1; `viewmodes` 1; a bare `0` 2.

## Load-time behaviour

1. **Check the header.** The resource type must be 0 and the version must be −5. N must be 1..8. Anything else fails the load. A file shorter than 164 bytes, or one that ends before its last level, fails as invalid data.
2. **Drop top levels.** The loader drops the first k levels, where:

   ```math
   k = \min\big(N - 1,\; \max(0,\; \mathrm{MipMapOffset} + \mathrm{GroupOffset}[g])\big)
   ```

   - g is the texture group. GroupOffset[g] counts only when g < 49; otherwise k uses MipMapOffset alone (still at least 0).
   - Both are console variables, default 0. The engine's own defaults for groups 0..3 are 0, 0, 1, 1.
   - **Groups 4 to 48 have no variable.** The engine's table of group offsets has only four entries (groups 0..3), but the loader accepts any g below 49 and reads entry g, so for g = 4..48 it reads whatever engine value follows the table in memory (Open question 4). Group 5, used by 28 files in `FX.rez`, is in that range. The value is not knowable from the reference; a reader should use 0 for groups 4..48 until a measurement settles it.
   - The game then rewrites GroupOffset1..3 from its "DetailLevel" option, index d = 0, 1, 2:

   | d | GroupOffset1 | GroupOffset2 | GroupOffset3 |
   |---|---|---|---|
   | 0 | 0 | 0 | 0 |
   | 1 | 0 | 1 | 1 |
   | 2 | 1 | 1 | 2 |

   - All three presets in the reference use d = 2, and the install's player profile holds `DetailLevel = 2`. So in practice FX textures in groups 1 and 2 load one level down and those in group 3 two levels down. Group 0, which is nearly everything, is untouched; group 5 is unknown (above).
   - **No in-game option changes it** (Open question 4). The graphics options screen has a quality preset (low, medium, high, user) and separate switches (shell casings, tracers, shadows, anisotropic and trilinear filtering, environment detail, shaders, bloom, effects detail, resolution, colour depth, anti-aliasing, gamma), but none of them writes `DetailLevel`, and every preset carries d = 2. The Steam profile (`PerformanceSetting = 3`, the user preset) also holds `DetailLevel = 2`. Only editing the profile or the console changes it. Levels skipped, k before the N − 1 clamp, with MipMapOffset 0:

   | Group | d = 0 | d = 1 | d = 2 (shipped, every option value) |
   |---|---|---|---|
   | 0 | 0 | 0 | 0 |
   | 1 | 0 | 0 | 1 |
   | 2 | 0 | 1 | 1 |
   | 3 | 0 | 1 | 2 |
   | 4–48 (5 occurs) | undefined (read past the table) | undefined | undefined |
   | ≥ 49 | 0 | 0 | 0 |
   - The dropped levels are skipped in the file, not read. The texture then reports level k as its full size, (W ≫ k) × (H ≫ k), but its original size stays known to the engine.
3. **Choose levels for the renderer.**
   - The starting level s is the first remaining level that fits the device's maximum texture size (default limit 16,384, so in practice level 0 of what was kept). If the card cannot take compressed data, s is also at least the extra-drop field at 0x1B, but never past the last level.
   - The renderer builds n levels from s:

   ```math
   n = \operatorname{clamp}\big(u = (\text{byte } 0x19 = 0\ ?\ 8 : \text{byte } 0x19),\; 1,\; (N - k) - s\big)
   ```

   So a file whose "levels to use" field is 1 shows only its top level, with no mipmapping, even when it stores more.
4. **Pick the device format.**
   - DXT data stays compressed when the card supports it. That is the normal case, and the "S3TCEnable" console variable defaults to on.
   - 32-bit data becomes a 32-bit ARGB texture unless the "prefer 16-bit" flag is set or the "32BitTextures" console variable is off. In those cases the choice goes, in this order: prefer 5551, then prefer 4444, then fullbright (5551), then plain 16-bit.
   - Our renderer can ignore the 16-bit choices: they only lose precision.

## Render behaviour from the header

The header holds only what this section lists. Translucency, additive blending, two-sided drawing and similar effects come from the world surface flags or the model's render style, not from the texture.

### Command string parsing

- The string is split into commands at `;` and at control characters (bytes 0..31 and 127).
- Each command is split into words at spaces. A `"` or `(` ... `)` pair groups spaces into one word, and `%` makes the next character literal.
- A keyword matches the first word of a command, ignoring case, as a whole word. `envmap` does not match `envmapalpha`.
- A search looks through all commands in the string and uses the first command whose first word matches.

### Linked textures

At most one of these takes effect. The engine tests them in the order below, and the first keyword found anywhere in the string wins:

| Keyword | Words | Effect |
|---|---|---|
| `DetailTex` | name | A detail texture, layered over the base texture (below) |
| `EnvMap` | name | An environment map (reflection) applied over the base texture |
| `EnvMapAlpha` | name | An environment map scaled by the base texture's alpha: alpha 255 shows the full reflection, alpha 0 none |
| `EnvBumpMap` | env name, bump name | Environment bump mapping |
| `EnvBumpMapNoFallback` | env name, bump name | The same, with no fallback on cards without support |
| `DOT3BumpMap` | bump name | DOT3 bump mapping |
| `DOT3EnvBumpMap` | env name, bump name | DOT3 bump mapping plus an environment map |
| `Effect` | shader id, then up to 4 names | A numbered effect shader with up to four extra textures |

- Each name is resolved like any other texture name (see the next section). In detail (Open question 11): the name is the command's second word exactly as written (a `"`-quoted word may hold spaces); it is handed to the client file manager as a client file and looked up in the mounted archive tree like every texture: a path from the game folder whose first component is the archive's root directory, `\` or `/` as separator, ASCII case ignored, and the extension as written. Nothing is added or removed: `textures\Etc\256xSprinkle01.dtx` resolves, `textures\Etc\256xSprinkle01` would not. The result is the shared texture for that name, so two links to the same file (or a texture linking to itself) give one texture object. The linked file is loaded with its own header (its own group, mip and format settings); its own command string is parsed when it loads, like any texture's.
- The **detail scale and rotation** come from the **base** texture's header (0x1E, 0x22), not from the detail texture's; the blend is the signed add (or multiply) under Detail textures below. In the install, the three `detailtex` users store scale field 0.0 (scale 1.0) except `TEXFX\ENVMAP\DUSTBEAM1.DTX` (7.0, so 8.0), whose link does not resolve; all three have rotation 0.
- A keyword with too few words makes the engine report an invalid file. The base texture has already been installed by then and stays usable.
- In the install only `detailtex`, `envmap` and `envmapalpha` occur.

### Alpha test

- `AlphaRef N` (any case) turns on alpha testing with reference value N, an integer 0..255. Missing or 0 means no alpha test.
- A pixel is drawn only when its alpha passes the test. For world geometry the test is alpha > N; elsewhere (models, sprites) it is alpha ≥ N.
- The test is separate from the linked-texture keywords above and is read even when one of them is present.
- `MipMapBias X` adds X (a float) to the texture's mip LOD bias, on top of the global "MipMapBias" console variable. It does not occur in the install.

### Fullbright

When flag bit 0 is set, the world renderer draws the surface in two passes:

- First the normal lit pass: texture × lightmap.
- Then the texture again, blended by its own alpha:

```math
\text{out} = \text{tex}_{rgb} \cdot \text{tex}_a + \text{lit} \cdot (1 - \text{tex}_a)
```

Where alpha is 255 the texture shows at full brightness whatever the lighting. Where alpha is 0 it is lit normally. Models and other paths also carry the flag through to their own fullbright handling (see Open questions). The texture is not blended with what lies behind it.

### Detail textures

- The detail texture is sampled with coordinates derived from the base texture's (u, v). The scale and rotation are the **base** texture's header fields, and the "DetailTextureScale" console variable (default 1.0) multiplies the scale:

```math
\sigma = \text{scale} \cdot \mathrm{DetailTextureScale},\quad \begin{pmatrix} u_d \\ v_d \end{pmatrix} = \sigma \begin{pmatrix} \cos\theta & -\sin\theta \\ \sin\theta & \cos\theta \end{pmatrix} \begin{pmatrix} u \\ v \end{pmatrix}
```

  Here θ is the rotation field in degrees converted to radians.
- It is combined with the base colour as a signed add when "DetailTextureAdd" is on (the default), and as a multiply when it is off:

```math
\text{add: } \text{out} = \operatorname{clamp}(\text{base} + \text{detail} - 0.5,\, 0,\, 1), \qquad \text{multiply: } \text{out} = \text{base} \cdot \text{detail}
```

  Alpha comes from the base texture.
- Detail texturing is skipped when the "DetailTextures" console variable is 0.

### Wrapping and filtering

The file has no addressing or filter fields. Every texture stage defaults to wrap (repeat) on both axes, and the engine switches to clamp only for its own full-screen effects. HUD and interface drawing chooses its own mode (Open questions). Filtering (bilinear, trilinear, anisotropic) comes from user options.

## How other files name a texture, and missing textures

- **Form of a name.** A texture is named by a path relative to the game folder: directory names, then the file name with its `.dtx` extension. Examples are the command-string targets `TEXFX\SKY\SKY20_RT.dtx` and `textures\etc\env_BD_01.dtx`, and the HUD's names in the client code.
  - The separator is `\`. Accept `/` too.
  - Case is ignored: the engine upper-cases names before comparing.
  - No extension is ever added. A name without `.dtx` does not match the file.
  - The first component is the archive's root directory (`TEXTURES`, `TEXFX`, `UI_HUD_COMMON`, ...). The mounted archive set behaves as one tree, as `rez_archive.md` describes.
- **Who names textures.** The world file lists names for its surfaces (world spec). Model skins are set by the game's model data and render styles (models spec). The HUD and menus use names in the game's data and code. Textures name each other in command strings.
- **When a name does not resolve,** the texture slot is left empty, and with the engine's debug level ≥ 1 a console message is printed:
  - **World surfaces** then try `textures\default_texture.dtx`. That file is **not** in the Steam install, so the surface ends with no texture.
  - **A linked texture** (detail, environment map) gets an empty link, but the base texture is still marked as using that kind of link. Three links in the install point at files that are not there:

  | Base texture | Missing link |
  |---|---|
  | `TEXFX\ENVMAP\DUSTBEAM1.DTX` | `TexFX\TextureScripts\Global_Dust1.dtx` |
  | two textures | `textures\Snow_valley\Window\512xSiloCWindow01.dtx` |
  | one texture | `TEXTURES\GRAYHAMMER\ETC\256XENV01.DTX` |

  - **Model skins** that do not resolve leave that skin slot empty.
- **A file that exists but fails to load** (a wrong version, or a zero-length file) leaves the texture with no data. The engine reports the load error and draws nothing from it.

## Edge cases

- **Zero-length files.** Two `.dtx` entries are 0 bytes long: `TEXTURES\DOOR\512XWINDOW01.DTX` (unkeyed) and one in a keyed archive. The reader must return a "too short" error, not panic.
- **"Levels to use" larger than N or 0.** Clamp, as in Load-time behaviour step 3. One file stores 41 with N = 8.
- **Self-link.** `TEXTURES\ETC\256XSPRINKLE01.DTX` names itself as its own detail texture, and `TEXTURES\ETC\SPRINKLE_RIVER.DTX` names it too. A loader must not recurse forever: resolve names to a shared handle first, then load once.
- **Non-square textures.** They are valid at any aspect ratio. The halving runs on each axis on its own, so in a deep chain one axis can reach 1 before the other.
- **Texture group ≥ 49.** Only the global MipMapOffset applies.
- **Command string with no terminator.** Use all 128 bytes.

## Quirks

- **Misspelled alpha test.** `TEXTURES\OBJECTS\GB_SPRINKLER.DTX` has the command `aipharef 128`. It matches no keyword, so that texture is drawn with **no** alpha test. Its cut-out pixels show as their DXT5 colour. Keep this.
- **DXT1 punch-through without alpha test.** DXT1 blocks in 3-colour mode give transparent black texels. Many DXT1 textures have some such blocks: 2 of the 4,096 blocks in the world texture of test case 1, and 1,750 of the 16,384 level-0 blocks in test case 9. (DXT3 and DXT5 never use this mode.) With no `alpharef` and no blending they draw as **black**, not as holes. Keep this.
- **High DetailLevel drops FX mips.** As shipped, DetailLevel 2 makes FX textures in groups 1 to 3 load one or two levels smaller. Whether to copy this is a choice; the default we recommend is to copy it, so that FX sprites look as they do in the game.
- **Effective detail scale 0.** One file stores −1.0, so all its detail coordinates collapse to (0, 0) and the detail texture becomes one flat colour. This file has no `detailtex`, so it does not show. Keep the formula as it is.

## Test cases

All on Steam build 25595979. Read each file with the `.rez` reader from the named archive (all unkeyed), by its path inside the archive. "Offsets" are where each level starts, from the start of the file. Flags are hex; group, "use" (byte 0x19) and the other block fields are decimal.

1. **World surface, DXT1.** `TEXTURES.rez` → `TEXTURES\METAL\256XRUSTYMETALPANNEL01.DTX`.
   - Size 43,844; version −5; 256 × 256; format 4 (DXT1); N = 6; flags 0x8; surface flags 20; group 0; use 6; command string empty.
   - Offsets: 164, 32,932, 41,124, 43,172, 43,684, 43,812; end 43,844.
2. **Model skin, DXT5, one level.** `GUNS_T_PV_SG.rez` → `GUNS_T_PV_SG\PV_SAIGA20K_VEPR_12.DTX` (a shotgun's first-person skin).
   - Size 1,048,740; 1024 × 1024; format 6; N = 1; flags 0x8; surface flags 0; use 1.
   - Offsets: 164; end 1,048,740.
3. **HUD element, 32-bit.** `ui_hud_common.rez` → `UI_HUD_COMMON\GUIDE\GUIDE_ITEM_MUTATION.DTX`.
   - Size 16,548; 64 × 64; format 3; N = 1; flags 0x8; use 1.
   - Offsets: 164; end 16,548.
   - Byte 3 of each pixel is alpha: 178 of its 4,096 pixels have byte 3 = 255.
4. **Alpha-tested, DXT5, non-square.** `PROPS.rez` → `PROPS\TREES\512XCOCOTREE01.DTX` (a palm tree).
   - Size 174,884; 512 × 256; format 6; N = 6; flags 0x8; surface flags 30; use 6; command `alpharef 128`, so the alpha test is on with reference 128.
   - Offsets: 164, 131,236, 164,004, 172,196, 174,244, 174,756; end 174,884.
5. **Fullbright, DXT5.** `TEXTURES.rez` → `TEXTURES\FLOOR\GB_CELLING_FB005.DTX`.
   - Size 11,044; 128 × 64; format 6; N = 4; flags 0x9 (fullbright + sections valid); surface flags 10; use 4; no command.
   - Offsets: 164, 8,356, 10,404, 10,916; end 11,044.
6. **DXT3, "use" = 0.** `FX.rez` → `FX\BEAMS\LASER_RED4.DTX`.
   - Size 11,044; 128 × 64; format 5; N = 4; flags 0x8; use 0, which means 8 clamped to 4, so all 4 levels are used.
   - Offsets: as in case 5.
7. **32-bit chain down to 1 × 2.** `PROPS.rez` → `PROPS\SAND_HOG\16XGAUGEBAR01.DTX`.
   - Size 844; 8 × 16; format 3; N = 4; flags 0x8.
   - Levels 8×16, 4×8, 2×4, 1×2 at offsets 164, 676, 804, 836; end 844.
8. **Detail fields, missing link, use < N.** `TEXFX.rez` → `TEXFX\ENVMAP\DUSTBEAM1.DTX`.
   - Size 21,924; 128 × 128; format 6; N = 4; flags 0x88 (prefer 4444 + sections valid); surface flags 40.
   - use 1, so the renderer builds only the top level.
   - Bytes 0x1E..0x21 hold 7.0, so the detail scale is 8.0; rotation 0.
   - Command `Detailtex TexFX\TextureScripts\Global_Dust1.dtx`. The target does not resolve, so the detail link is empty.
   - Offsets: 164, 16,548, 20,644, 21,668; end 21,924.
9. **Eight levels to 4 × 4, use = 41.** `FX.rez` → `FX\CUTSCENE\FT_ZOMBIE\CM_VST_HV_DE_NX_YK.DTX`.
   - Size 174,924; 512 × 512; format 4; N = 8; use 41, which clamps to 8.
   - Offsets: 164, 131,236, 164,004, 172,196, 174,244, 174,756, 174,884, 174,916; end 174,924.
10. **Group mip skip.** Any `FX.rez` texture with group 3 and N ≥ 3 loads with k = 2 at DetailLevel 2, so its reported size is (W ≫ 2) × (H ≫ 2). A group-0 texture under the same settings has k = 0.
11. **Zero-length file.** `TEXTURES.rez` → `TEXTURES\DOOR\512XWINDOW01.DTX` is 0 bytes. The reader returns a "too short" error.
12. **Misspelled command.** `TEXTURES.rez` → `TEXTURES\OBJECTS\GB_SPRINKLER.DTX`.
    - Size 5,604; 64 × 64; format 6; N = 4; command `aipharef 128`, so the alpha test is **off**.
13. **Self-linked detail texture.** `TEXTURES.rez` → `TEXTURES\ETC\256XSPRINKLE01.DTX`.
    - 256 × 256; format 6; N = 6; size 87,524; command `detailtex textures\Etc\256xSprinkle01.dtx`. Resolving it gives the texture itself, and loading must finish.
14. **Whole-install sweep.** Over all unkeyed archives:
    - 9,658 `.dtx` entries.
    - 9,657 parse with version −5 and size exactly 164 + Σ S. 1 is empty.
    - Format counts: 3,237 / 4,068 / 457 / 1,895 for codes 3 / 4 / 5 / 6.
    - Every flags value includes 0x8, and every section count is 0.
15. **Mip skip by group, shipped settings.** DetailLevel 2, MipMapOffset 0, textures with N = 4 → k = 0 for group 0, 1 for groups 1 and 2, 2 for group 3, 0 for group 49 or above. Group 3 with N = 2 → k = 1 (clamped to N − 1). The same group-3 texture with DetailLevel 0 → k = 0.
16. **Options do not change it.** Switching the quality preset between low, medium, high and user in the graphics options → GroupOffset1, 2, 3 stay 1, 1, 2.
17. **Interpolated colour rounding.** DXT1 block c0 = 0xF800 (red 31 → 255), c1 = 0x0800 (red 1 → 8), index 2 → red 173 when rounding to nearest (the reading to keep), 172 on the engine's software path; index 3 → red 90 either way.
18. **Software-path widening.** c0 = 0x8000 (red 16) → 132 by bit repetition (the reading to keep), 131 on the engine's software path.
19. **DXT5 alpha rounding.** a0 = 255, a1 = 0 → A2 = 219, A3 = 182, A4 = 146, A5 = 109, A6 = 73, A7 = 36 rounding to nearest; the software path gives 218, 182, 145, 109, 72, 36.
20. **Software path, DXT3 with c0 ≤ c1.** A DXT3 block with c0 = 0x0000 and c1 = 0xFFFF, colour index 3, explicit alpha 15 → the software path gives black with alpha 255 (3-colour mode); the reading to keep gives the 2/3 entry (170, 170, 170) with alpha 255.
21. **Link resolution.** `TEXTURES\ETC\SPRINKLE_RIVER.DTX` (command `detailtex textures\Etc\256xSprinkle01.dtx`) and `TEXTURES\ETC\256XSPRINKLE01.DTX` (self-link) → both links give the one texture object of `TEXTURES\ETC\256XSPRINKLE01.DTX`. The detail scale used for SPRINKLE_RIVER's surfaces is SPRINKLE_RIVER's own field (stored 0.0, scale 1.0).
22. **Link without extension.** A command `detailtex textures\Etc\256xSprinkle01` (no `.dtx`) → the link is empty, the base texture draws without detail.

## Synthetic fixtures

Each fixture is one 4 × 4 texture with N = 1. Its header is the 164 bytes below, with only the format code changed. A unit test writes the header, appends the pixel bytes, then checks the decoded RGBA.

**Header** (all 164 bytes; any byte not listed is 0):

| Offset | Bytes | Field |
|---|---|---|
| 0x00 | `00 00 00 00` | resource type 0 |
| 0x04 | `04 00` | width 4 |
| 0x06 | `04 00` | height 4 |
| 0x08 | `FB FF FF FF` | version −5 |
| 0x0C | `01 00` | N = 1 |
| 0x0E | `00 00` | no sections |
| 0x10 | `08 00 00 00` | flags 0x8 |
| 0x14 | `00 00 00 00` | surface flags 0 |
| 0x18 | `00` | group 0 |
| 0x19 | `01` | use 1 |
| 0x1A | `03`, `04`, `05` or `06` | format code |
| 0x1B..0xA3 | zeros | rest of the block, empty command string |

File sizes: 32-bit 228 bytes, DXT1 172, DXT3 180, DXT5 180.

### F1. 32-bit (code 3), 64 pixel bytes from offset 164

Rows 0 and 2 store the same bytes, and so do rows 1 and 3. Each cell shows the stored bytes (B G R A), then the decoded RGBA.

| Row | x = 0 | x = 1 | x = 2 | x = 3 |
|---|---|---|---|---|
| 0, 2 | `00 00 FF FF` → (255,0,0,255) | `00 FF 00 FF` → (0,255,0,255) | `FF 00 00 FF` → (0,0,255,255) | `FF FF FF FF` → (255,255,255,255) |
| 1, 3 | `00 00 00 FF` → (0,0,0,255) | `00 00 00 00` → (0,0,0,0) | `80 80 80 80` → (128,128,128,128) | `56 34 12 78` → (0x12,0x34,0x56,0x78) |

### F2. DXT1 (code 4), 4-colour block

- Block bytes: `00 F8 1F 00 E4 00 55 1B`.
- c0 = 0xF800, which is red (255,0,0); c1 = 0x001F, which is blue (0,0,255). c0 > c1, so 4-colour mode.
- P2 = (170,0,85) and P3 = (85,0,170). All alphas are 255.

| Row | Index byte | Indices x = 0..3 | Decoded RGB |
|---|---|---|---|
| 0 | `E4` | 0 1 2 3 | (255,0,0) (0,0,255) (170,0,85) (85,0,170) |
| 1 | `00` | 0 0 0 0 | red ×4 |
| 2 | `55` | 1 1 1 1 | blue ×4 |
| 3 | `1B` | 3 2 1 0 | (85,0,170) (170,0,85) (0,0,255) (255,0,0) |

### F3. DXT1 (code 4), 3-colour block with transparency

- Block bytes: `1F 00 00 F8 F4 FF 50 05`.
- c0 = 0x001F (blue), c1 = 0xF800 (red). c0 ≤ c1, so 3-colour mode: index 3 is (0,0,0,0).

| Row | Index byte | Indices | Decoded RGBA |
|---|---|---|---|
| 0 | `F4` | 0 1 3 3 | (0,0,255,255) (255,0,0,255) (0,0,0,0) (0,0,0,0) |
| 1 | `FF` | 3 3 3 3 | (0,0,0,0) ×4 |
| 2 | `50` | 0 0 1 1 | blue, blue, red, red (alpha 255) |
| 3 | `05` | 1 1 0 0 | red, red, blue, blue (alpha 255) |

### F4. DXT3 (code 5)

- Block bytes: `50 FA FF FF FF FF FF FF FF FF 00 00 00 00 00 39`.
- Alpha half: row 0 nibbles are 0, 5, 10, 15, giving alphas 0, 85, 170, 255. Rows 1 to 3 are all 15, giving 255.
- Colour half: c0 = 0xFFFF (white), c1 = 0x0000 (black). P2 = (170,170,170) and P3 = (85,85,85).
- Rows 0 to 2 use index 0. Row 3 uses indices 1, 2, 3, 0 (index byte `39`).

| Row | Decoded RGBA, x = 0..3 |
|---|---|
| 0 | (255,255,255,0) (255,255,255,85) (255,255,255,170) (255,255,255,255) |
| 1, 2 | (255,255,255,255) ×4 |
| 3 | (0,0,0,255) (170,170,170,255) (85,85,85,255) (255,255,255,255) |

### F5. DXT5 (code 6)

- Block bytes: `00 FF 88 C6 FA 49 92 24 E0 07 00 00 00 00 00 00`.
- a0 = 0 and a1 = 255. a0 ≤ a1, so the 6-value mode: A0..A7 = 0, 255, 51, 102, 153, 204, 0, 255.
- Alpha indices: row 0 is 0 1 2 3, row 1 is 4 5 6 7, rows 2 and 3 are all 1.
- Colour half: c0 = 0x07E0, which is green (0,255,0); c1 = 0; every index is 0, so every texel is green.

| Row | Decoded alpha, x = 0..3 (RGB is (0,255,0) everywhere) |
|---|---|
| 0 | 0, 255, 51, 102 |
| 1 | 153, 204, 0, 255 |
| 2, 3 | 255 ×4 |

### F6. Mip chain layout (header only)

- A header with W = 16, H = 8, N = 4 and format 4 has levels 16×8, 8×4, 4×2, 2×1. The last two are under 4 pixels, so the reader must reject the file. With N = 2 the file is valid: offsets 164 and 228, end 244.
- The same 16 × 8 with format 3 and N = 4 has offsets 164, 676, 804, 836 and end 844.

### F7. Rejections

- Version −4 (bytes `FC FF FF FF`) is rejected as a wrong version.
- Resource type 1 is rejected as invalid data.
- N = 0 and N = 9 are rejected.
- A valid header whose file is one byte shorter than the end of its last level is rejected as truncated.

### F8. Command strings

These go in the 128-byte field from 0x24.

| Command string | Alpha ref | Link |
|---|---|---|
| `alpharef 128` | 128 | none |
| `ALPHAREF 1` | 1 | none |
| `aipharef 128` | none | none |
| `alpharef 0` | none | none |
| `envmapalpha TEXFX\SKY\SKY20_RT.dtx` | none | environment map scaled by alpha, target `TEXFX\SKY\SKY20_RT.dtx` |
| `envmap a.dtx` | none | plain environment map |
| `alpharef 128; detailtex a.dtx` | 128 | detail texture `a.dtx` |
| `detailtex a.dtx` then 0x00, then leftover bytes `b.dtx` | none | detail `a.dtx`; the bytes after 0x00 are ignored |

## Open questions

1. **Keyed archives.** About 58% of the `.dtx` files (character, attachment and weapon skins, most menu screens, item images) are in `V2_RESOURCES_KEY` archives. They could not be decrypted here, so it is assumed, not checked, that their plaintext has the same layout. The sizes fit. Re-run test case 14 on the keyed archives once the coordinator's box has the V2 key.
2. **Palette channel order.** For format code 7, the 4-byte palette entries are read in the engine as alpha, red, green, blue at increasing addresses. That has not been confirmed on a real palettised file, because the install has none and the runtime cannot load the palette anyway. It is low priority.
3. **Section header size.** The 32-byte size (25 bytes of text, 3 of padding, then the length) is inferred from the in-memory layout with default alignment. No file in the install has a section to check it against.
4. **Is the shipped DetailLevel really 2?** It is in all three presets and the install's player profile, so groups 1 to 3 lose top mips. In the live game, check whether an FX sprite of group 3 (for example one of the 40 in `FX.rez`) looks blurrier than its file, and whether the in-game texture quality option changes the "DetailLevel" value. **Answered (mostly):** there is no in-game texture quality option; nothing in the graphics options writes DetailLevel, every preset carries 2, and the Steam profile holds 2, so groups 0..3 skip 0, 1, 1, 2 levels whatever the options say (table in Load-time behaviour, cases 15–16). **Still open: groups 4..48.** The engine reads their offset from beyond its four-entry table, so the value is not in the reference. Measurement: in the live game, open the console and look at a group-5 texture (28 in `FX.rez`) through any effect that shows it at a known size, or compare a screenshot of it with its file's level 0 and level 1; whether it is one level down tells the value. The group-3 check above still confirms the d = 2 reading.
5. **Untextured world surfaces.** When a world surface's texture and the `textures\default_texture.dtx` fallback are both missing, it is not settled what the renderer draws: white, flat lit, or nothing. Check in game on a surface whose texture name does not resolve, if the world spec finds one.
6. **Fullbright outside the world.** For models and sprites, the reference passes the fullbright bit through, but this session did not trace how each path uses it. The models spec should settle it.
7. **HUD wrap or clamp.** The interface drawing calls choose their own addressing. The HUD spec should say which.
8. **The renderer in the Steam build.** The behaviour above follows the reference's Direct3D 9 renderer. The Steam client may ship a later renderer, for example one with deferred lighting: the reference also contains a path that turns the fullbright pass off under deferred lighting. If so, fullbright and detail blending may differ in detail.
9. **`viewmodes` command.** One texture (`FX\TEST\LIGHTNING_1.DTX`) has `ViewModes Nightvision Normal;`. The engine's texture loader ignores it. Whether the game reads it anywhere is unknown, and it matters only to a night-vision feature.
10. **DXT interpolation rounding.** Implementers round the 1/3 and 2/3 colours to nearest; does the game truncate, and what about the alpha block? **Answered:** what players see is decoded by the graphics card (compression is on by default and on every current card), which rounds to within one step; keep round to nearest and bit-repeating widening. The engine's own software decoder, used only when the card cannot take DXT, truncates everywhere (colour entries, midpoint, DXT5 alpha entries), widens by ⌊c·255/max⌋ and picks the 3-colour mode by c0 ≤ c1 even for DXT3/DXT5. Applies in Format codes and pixel layouts (Rounding) and cases 17–20.
11. **Detail and environment links.** How a command string names its detail texture or environment map, how the name resolves, and the detail scale and blend. **Answered:** the keyword's second word, as written, is resolved as a client file in the mounted archives like any texture name (no extension added, ASCII case ignored, either separator) and becomes a shared texture; the detail scale and rotation are the base texture's header fields times `DetailTextureScale`, and the blend is the signed add (`DetailTextureAdd` 1, default) or multiply. Applies in Render behaviour (Linked textures, Detail textures) and cases 21–22.
