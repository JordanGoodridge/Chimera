# Combat Arms: world file (.dat)

Source basis: reference material the user pointed the spec session to on the engine's world loading (shared, client, server and renderer sides) and on the tool that compiles maps into world files. Checked by parsing every plaintext world file in the Steam install on the Windows PC (Steam build id 25595979) and in an older regional client, as numbers and structure only. A follow-up session added the engine's collision and ray-intersection code, the world renderer's fixed-function shaders, sky and light-group code, the game's volume, world-model, light-group, sky and world-properties objects, the network file list, and a census of the older regional client's 65 maps.
Status: draft

Sections: Summary; Units and conventions; Versions in the install; File layout; Header; World info string, extents and offset; World tree; World models (BSPs); Objects; Blind object data; Light grid; Collision blockers and particle blockers; Render data; Lightmaps; What is drawn and what is collision only; Sky; Second textures; Moving world models and volumes; Map identity; Other contents; Coordinate conversion; Edge cases; Quirks; Synthetic fixtures; Test cases; Open questions.

---

## Summary

- A world file is the compiled map. It starts with a 60-byte header: a version (85) and six absolute offsets to the later blocks. Then come a text info string, the world extents, the object quadtree layout and the world models (BSP trees with planes, polygons and vertices), followed by the blocks the header points at: objects, blind object data, light grid, collision blockers, particle blockers and render data.
- Geometry is stored twice. The **BSP world models** carry planes, polygons and vertices for collision, ray hits and surface-type queries, with no texture coordinates. The **render data** carries ready-to-draw vertex and triangle lists, grouped by texture and shader into sections, with texture and lightmap coordinates and the lightmaps themselves.
- Everything is little-endian, tightly packed (no alignment or padding), and read sequentially. Only the six header offsets allow seeking.
- Objects (spawn points, lights, sounds, doors, pickups, sky markers, game controllers) are a list of class names, each with a typed property list.

## Units and conventions

- Integers: u8, u16, u32 (unsigned) and i32 (signed), little-endian. Reals: IEEE-754 single precision ("f32"), little-endian.
- vec3: three f32 in the order x, y, z (12 bytes). plane: a vec3 normal followed by an f32 distance d (16 bytes); a point p lies on the plane when n·p = d, and d is stored this way in every block of the file (BSP planes, blockers, particle blockers, sky portals, occluders).
- lstring: a u16 byte count followed by that many bytes, with **no** terminator. Text is 8-bit. No byte above 0x7F occurs in any name or string of the 65 maps of the older regional client (class and property names, string properties, world-model and texture names, render section names); the engine keeps them as plain 8-bit strings with no code-page conversion and compares file names without regard to ASCII case (Open question 16). A reader should keep the bytes, compare them ASCII-case-insensitively where the engine does, and may decode them as Latin-1 for display. This is the string form everywhere except in the BSP texture-name list, which uses NUL-terminated strings (see World models).
- Offsets are absolute byte offsets from the start of the world file (the bytes the archive reader returns for the `.DAT` entry).
- Distances are LithTech world units ("u"); the file has no unit of its own (see Open questions for the real-world scale).
- Axes: +Y is up, +X is right and +Z is forward, a left-handed frame. Angles are radians. See Coordinate conversion.
- Colours in object properties and the world info string are 0–255 per channel stored as reals; colours inside the render data are 0–1 reals or bytes as noted.

## Versions in the install

- The Steam install keeps its maps in five archives: WORLDS, WORLDS2, WORLDS3, WORLDS4 and WORLDS5 (105 `.DAT` files in total). WORLDS2 (19 maps) and WORLDS3 (14 maps) are plain archives. WORLDS, WORLDS4 and WORLDS5 (72 maps) are V2-keyed archives (see rez_archive.md), whose payloads this session could not decrypt.
- **All 33 plaintext Steam maps have version 85.** They all parse with this spec to exactly their end of file. So do all 65 maps of the older regional client, whose world archives are all plain.
- Of the 72 encrypted Steam maps, 29 have exactly the size of a version-85 plaintext copy in the older client, 7 differ in size, and 36 have no counterpart. Of the 29 plaintext maps the Steam build and the old client share, 26 are byte-identical. Every encrypted map is very probably version 85 too, but this is unverified (Open questions).
- **Telling versions apart.** Version 85 is the last of a numbered series; the map compiler's history numbers earlier world layouts from 33 upwards. A reader for this spec:
  1. Reads the u32 at offset 0 and rejects the file unless it is 85. A still-encrypted payload fails here, because its first four bytes are effectively random.
  2. Checks that the six header offsets are non-decreasing in header order, that each one is at least 64 and at most the file size, and that the info-string length fits in the file.
  3. Checks that parsing each block ends exactly where the next block starts (see File layout); every observed file passes this check.
- A version-85 world written by another game on the same engine may well be byte-compatible, but this spec does not promise it.

## File layout

Blocks in file order. "Next" means the offset of the following block in this table.

| # | Block | How a reader finds it | Size |
|---|---|---|---|
| 1 | Header | offset 0 | 60 bytes |
| 2 | World info string | offset 60 | 4 + L bytes |
| 3 | Extents and world offset | right after 2 | 36 bytes |
| 4 | World tree layout | right after 3 | 32 + ceil(N/8) bytes, where N = tree node count |
| 5 | World models | right after 4 | up to the object-list offset |
| 6 | Object list | header offset 0 | up to the blind-data offset |
| 7 | Blind object data | header offset 1 | up to the light-grid offset |
| 8 | Light grid | header offset 2 | up to the blocker offset |
| 9 | Collision blockers | header offset 3 | up to the particle-blocker offset |
| 10 | Particle blockers | header offset 4 | up to the render-data offset |
| 11 | Render data, then world light groups | header offset 5 | to the end of the file |

- Blocks are contiguous. In every observed file, sequential parsing of block 5 ends exactly at header offset 0, and so on for each block, ending exactly at the file size.
- The engine itself reads blocks 2–5 sequentially, reads block 6 sequentially from right after block 5 (it does not seek to header offset 0 for that pass), and seeks for blocks 6–11 elsewhere. A reader should seek by the header offsets and may verify the sequential ends against them.

## Header

| Offset | Size | Type | Meaning |
|---|---|---|---|
| 0 | 4 | u32 | Version. Must be 85. |
| 4 | 4 | u32 | Offset of the object list |
| 8 | 4 | u32 | Offset of the blind object data |
| 12 | 4 | u32 | Offset of the light grid |
| 16 | 4 | u32 | Offset of the collision blockers |
| 20 | 4 | u32 | Offset of the particle blockers |
| 24 | 4 | u32 | Offset of the render data |
| 28 | 32 | u32 × 8 | Reserved. The engine reads the first two as a "packer kind" and "packer version" and ignores all eight. The compiler writes zeros, and all eight are 0 in every observed file. |

## World info string, extents and offset

At offset 60:

| Offset (relative) | Size | Type | Meaning |
|---|---|---|---|
| 0 | 4 | u32 | L, the info string length in bytes |
| 4 | L | bytes | Info string, no terminator |
| 4+L | 12 | vec3 | World extents minimum |
| 16+L | 12 | vec3 | World extents maximum |
| 28+L | 12 | vec3 | World offset |

- **Info string.** Console-style statements separated by `;`, each a keyword followed by space-separated arguments. Keywords seen: `ambientlight r g b` (0–255 each; the engine clamps to 0..255 and uses it as the world's ambient light; missing means 0 0 0) and `lmgridsize n` (the lightmap texel size used at compile time; informational). The string may be empty (L = 0). Keywords are matched without regard to case; files use lower case.
- **Extents.** The bounding box of the whole world. The engine precomputes 1 / (max − min) per axis from it. In every observed file it is symmetric about the origin and equals the world-tree box and the light-grid origin.
- **World offset.** The compiler can recentre a map on the origin. This vector is the amount it removed: a position in the original editor space is the file position plus this offset. The engine keeps it for texture-animation scripts. Gameplay uses file positions as they are. 15 of the 33 Steam maps have a non-zero offset.

## World tree

The world tree is a quadtree over X and Z (the full Y range in every node) that the engine uses to bucket objects for visibility and queries. The file stores only its shape.

| Offset (relative) | Size | Type | Meaning |
|---|---|---|---|
| 0 | 12 | vec3 | Root box minimum |
| 12 | 12 | vec3 | Root box maximum |
| 24 | 4 | u32 | N, total node count **including** the root |
| 28 | 4 | u32 | Terrain depth. Unused; 0 in every observed file. |
| 32 | ceil(N/8) | bits | One bit per node in pre-order (node, then its four children in order). 1 = subdivided. Bits are taken from the least significant bit of each byte upward. |

- Read a bit for the root. If it is 1, the node has four children, each read recursively in order 0, 1, 2, 3. Child k covers this quarter of the parent box, with c the parent centre:

| Child | X range | Z range |
|---|---|---|
| 0 | min.x … c.x | min.z … c.z |
| 1 | min.x … c.x | c.z … max.z |
| 2 | c.x … max.x | min.z … c.z |
| 3 | c.x … max.x | c.z … max.z |

- The number of nodes produced must equal N, which is 1 + 4 × (number of 1 bits). The last byte may be partly unused; its spare high bits are 0.
- Observed trees are complete. N = 5461 is a full tree of depth 6 below the root, and N = 21845 a full tree of depth 7.

## World models (BSPs)

| Size | Type | Meaning |
|---|---|---|
| 4 | u32 | M, number of world models |
| … | record × M | World model records |

Each world model record starts with a **u32 that is always 0** (the engine skips it), followed by the BSP:

### BSP header

| Size | Type | Meaning |
|---|---|---|
| 4 | u32 | World-model flags (only the low 16 bits are meaningful) |
| 2+n | lstring | Name (at most 63 characters are kept) |
| 4 | u32 | P, point count |
| 4 | u32 | Q, plane count |
| 4 | u32 | S, surface count |
| 4 | u32 | User-portal count. Must be 0; a file with any is rejected. |
| 4 | u32 | G, polygon count |
| 4 | u32 | Leaf count. 0 in every observed file (see Edge cases). |
| 4 | u32 | Total polygon-vertex references, the sum of all polygon vertex counts. Consistency check only. |
| 4 | u32 | Visibility list size. Unused, 0. |
| 4 | u32 | Leaf list count. Unused, 0. |
| 4 | u32 | K, node count |
| 12 | vec3 | Bounding box minimum |
| 12 | vec3 | Bounding box maximum |
| 12 | vec3 | Translation: the centre of the bounding box for ordinary world models, (0, 0, 0) for the physics BSP |
| 4 | u32 | T_len, total bytes of the texture-name list |
| 4 | u32 | T, texture count |

World-model flags:

| Bit | Value | Meaning |
|---|---|---|
| 1 | 0x02 | Movable: the world model belongs to an object that can move or rotate (doors, platforms, breakables and so on) |
| 2 | 0x04 | Main world |
| 4 | 0x10 | Physics BSP: the static level geometry, used for collision. Exactly one per file. |
| 5 | 0x20 | Visibility BSP. At most one; none in any observed file. |

Observed values: 0x02 on every ordinary world model, and 0x14 (main world + physics BSP) on exactly one world model, named `PhysicsBSP`, which is always the **last** record.

### BSP arrays

After the BSP header, in this order:

| Array | Count | Element size | Element layout |
|---|---|---|---|
| Texture names | T | variable | NUL-terminated string. The sum of the lengths including terminators equals T_len. |
| Polygon vertex counts | G | 1 | u8 vertex count of polygon i, in polygon order. 3..18 observed; the engine's limit is 40. The engine does not check it: a polygon of more than 40 vertices overruns its fixed 40-entry read buffer (undefined). A reader should refuse it. |
| Leaves | leaf count | variable | Skipped (Edge cases). None in observed files. |
| Planes | Q | 16 | plane |
| Surfaces | S | 8 | u32 surface flags, u16 texture index into the texture names, u16 surface type |
| Polygons | G | 8 + 4·v | u32 surface index, u32 plane index, then v u32 point indices (v from the vertex-count array) |
| Nodes | K | 14 | u32 polygon index, u16 unused (0), i32 back child, i32 front child |
| Points | P | 12 | vec3 |

Then two trailing fields:

| Size | Type | Meaning |
|---|---|---|
| 4 | i32 | Root node index |
| 4 | u32 | Section count. Must be 0 (former terrain sections); a file with any is rejected. |

Rules and meanings:

- **Validation (engine).** Polygon surface index < S, plane index < Q, every point index < P, node polygon index < G. Child and root indices are either −1, −2 or a node index < K.
- **Points** are in world space, at the world model's placed location; nothing in the BSP is local to the model. The object that owns a movable world model carries its own position (its `Pos` property), which generally differs from the BSP translation.
- **Planes and polygons.** A polygon lies in its plane: n·p ≈ d for its points (observed error below 0.001 u). Its vertex order is such that (v1 − v0) × (v2 − v0), computed with the usual component formula, points along n. Across all 33 plaintext Steam maps this holds for 2,003,330 polygons. 39 have a degenerate first triangle and 22 (slivers) point the other way, so a reader should take the facing from the plane, not from the vertices.
- **Surface type.** The u16 after the texture index is a game surface type id (material) used for footsteps and impact effects. 0 is unknown, and values seen run up to about 204. Its meaning belongs to the game's surface table, not to this file.
- **Texture names** are paths into the texture archives (for example a `.dtx` path) or the word `Default`. The render data names its own textures, so the BSP list is only needed for surface queries.

Surface flags (u32). Our names; the bit numbers are the data.

| Bit | Meaning | Seen |
|---|---|---|
| 0 | Solid | yes |
| 1 | Removed by the compiler ("nonexistent"); left set on some surfaces | yes |
| 2 | Invisible (not drawn) | yes |
| 3 | Translucent | no |
| 4 | Sky portal | yes |
| 5 | Full-bright | no |
| 6 | Flat shaded | yes |
| 7 | Lightmapped | yes |
| 8 | Do not subdivide (compiler hint) | yes |
| 9 | Hull maker (compiler visibility hint) | no |
| 10 | Particle blocker (compiler; such polygons go to the particle-blocker block) | no |
| 11 | Lit only by the global directional light | no |
| 12 | Gouraud shaded (vertex lit) | yes |
| 13 | Openable portal | no |
| 15 | Panning sky overlay | no |
| 17 | Player blocker (compiler; such polygons go to the collision-blocker block) | no |
| 18 | Terrain occluder | no |
| 19 | Ambiguous: "split render blocks here" to the compiler, "additive blend" to the engine | no |
| 21 | Visibility blocker (becomes an occluder in the render data) | yes |
| 22 | Not a step: movement must not step up onto it | yes |
| 23 | Mirror | no |
| 24 | Receives light | yes |
| 25 | Receives shadows | yes |
| 26 | Receives sunlight | yes |
| 28 | Receives shadow mesh | yes |
| 29 | Casts shadow mesh | yes |
| 30 | Clips light (casts shadows at compile time) | yes |

- **Nodes and point location.** Each node splits space by the plane of its polygon. Back child = index 0 of the two, front child = index 1. Index −1 is the "in" leaf and −2 the "out" leaf. To locate a point p in a BSP:

```math
\text{start at the root};\quad s = n_{\text{poly}} \cdot p - d_{\text{poly}};\quad \text{go to the front child if } s > 0,\ \text{else the back child};\quad \text{repeat until index } -1\ (\text{in}) \text{ or } -2\ (\text{out})
```

- A point exactly on a plane (s = 0) goes to the back. In the physics BSP, "in" is open space inside the level and "out" is solid or outside. In the measured maps, every spawn point resolved to "in" (apart from a couple placed inside geometry), and walking straight down from a spawn point reaches "out" at the floor. The engine answers "is this point inside the world" with exactly this walk over the physics BSP.
- The engine classifies each node plane once: if a normal component exceeds 0.99999 in magnitude, the plane is treated as axis-aligned (±X, ±Y, ±Z) for faster tests, in that priority order. This is an optimisation only.
- **Polygon handles.** The engine names a polygon by (world-model index in file order, polygon index). The render triangles refer to polygons this way (Render data).

## Objects

At header offset 0:

| Size | Type | Meaning |
|---|---|---|
| 4 | u32 | Object count |
| … | record × count | Object records |

Object record:

| Size | Type | Meaning |
|---|---|---|
| 2 | u16 | R, byte length of the rest of the record (everything after this field) |
| 2+n | lstring | Class name (at most 255 characters) |
| 4 | u32 | Property count |
| … | property × count | Properties |

Property:

| Size | Type | Meaning |
|---|---|---|
| 2+n | lstring | Property name (at most 255 characters) |
| 1 | u8 | Type code |
| 4 | u32 | Editor flags (see below; no runtime meaning) |
| 2 | u16 | D, data length in bytes |
| D | bytes | Data, encoded by type |

Property types:

| Code | Type | D | Encoding |
|---|---|---|---|
| 0 | string | 2 + n | An lstring: u16 n, then n bytes. Can be empty (D = 2) and can contain line breaks. |
| 1 | vector | 12 | vec3 |
| 2 | colour | 12 | vec3 of r, g, b, each 0–255 |
| 3 | real | 4 | f32 |
| 4 | flags | 4 | **f32**; the engine converts it to a 32-bit integer by truncation toward zero |
| 5 | boolean | 1 | u8, non-zero is true |
| 6 | long integer | 4 | **f32**, converted like flags |
| 7 | rotation | 16 | Euler angles as three f32 (pitch about X, yaw about Y, roll about Z, radians) and one f32 that is always 0 |

- R covers the class name, property count and all properties exactly, so a reader can skip an object it does not want by jumping R bytes. Observed files are exact.
- Unknown type codes: the engine ignores the property but still consumes its data. A reader should skip D bytes.
- **Malformed records** (Open question 16). The engine reads the object list strictly in sequence: it reads R but never uses it to seek, so an R that disagrees with the content goes unnoticed and the next object is read from right after the last property. For every property it reads exactly D bytes, then interprets the leading bytes by type; the size checks exist only in debug builds. So a property longer than its type (a real with D = 8) uses its first bytes and skips the rest, and one shorter than its type (a real with D = 2) is read past its data (undefined). A string property is read as an lstring inside its D bytes. Running out of file anywhere fails the load as an invalid world. A reader should: refuse D smaller than the type's size; accept D larger and use the leading bytes; parse by content and treat an R mismatch as a warning (or refuse it; no observed file has one).
- **Rotation to matrix.** The engine builds the matrix as yaw, then pitch, then roll (R = R_y(yaw) · R_x(pitch) · R_z(roll)), with these columns (c = cos, s = sin; y = yaw, p = pitch, r = roll):

```math
\text{right} = \begin{pmatrix} c_r c_y + s_r s_p s_y \\ s_r c_p \\ -c_r s_y + s_r s_p c_y \end{pmatrix},\quad
\text{up} = \begin{pmatrix} -s_r c_y + c_r s_p s_y \\ c_r c_p \\ s_r s_y + c_r s_p c_y \end{pmatrix},\quad
\text{forward} = \begin{pmatrix} c_p s_y \\ -s_p \\ c_p c_y \end{pmatrix}
```

  So yaw 0 faces +Z, positive yaw turns toward +X, and positive pitch looks down.
- **Editor flags** (data the editor stored with each property): bit 0 hidden, 1 radius, 2 dimensions, 3 field of view, 4 local dimensions, 5 group owner, bits 6–11 a group number, 12 field-of-view radius, 13 object link, 14 file name, 15–16 curve tangents, 17 static list, 18 dynamic list, 19 composite, 20 distance, 21 model, 22 orthographic frustum, 23 notify change, 24 event, 25 texture effect. A reader may ignore them.
- **Common properties.** Nearly every object has `Name` (string, unique in the map), `Pos` (vector, world position in file space), `Rotation` (rotation) and `RenderGroup` (long). Objects that use a world model have a `Name` **equal to a BSP world-model name**; that is how a door or platform finds its geometry. In every Steam map, every ordinary world-model name is the `Name` of some object.
- **How common things are expressed** (classes and properties are data; their gameplay meaning belongs to other specs):

| Thing | Class (and key properties) |
|---|---|
| Player spawn point | `GameStartPoint`: `Pos`, `Rotation` (yaw is the facing), `GameType` (string, the mode the point serves, e.g. `Team Death Match`, `FreeForAll`, `Capturetheflag`, `Quarantine`), `Teams` (string: `JSF`, `Creed` or `<none>`), `Spawn` (long), `Locked` (boolean), `Command` (string) |
| Point light | `Light`: `LightRadius` (real), `LightColor` (colour), `BrightScale` (real), `Attenuation` (string: `D3D`, `Quartic` or `Linear`), `AttCoefs` and `AttExps` (vectors), `LightObjects`, `FastLightObjects`, `CastShadows` (booleans), `ConvertToAmbient`, `ObjectBrightScale` (reals), `LightGroup` (string) |
| Spot light | `DirLight`: as Light, with `InnerColor` (colour) and `FOV` (real, degrees); the direction is the forward vector of `Rotation` |
| Sun | `StaticSunLight`: `InnerColor`, `BrightScale`, `ConvertToAmbient`, `ObjectBrightScale`; direction from `Rotation` |
| Switchable light group | `LightGroup`: `Name` (the group name), `StartOn` (boolean), `StartColor` (colour) |
| Sound | `SoundFX`: `Sound` (string, sound file path), `InnerRadius`, `OuterRadius` (reals), `Volume` (long), `Loop`, `Ambient`, `StartOn` (booleans) |
| Door, moving geometry | `RotatingDoor`, `SlidingDoor`, `RotatingWorldModel`, `SlidingWorldModel`, `SpinningWorldModel`, `WorldModel`, `Breakable` and others, each tied to the world model of the same `Name`; motion in properties such as `RotationAngles`, `MoveDir`, `MoveDist`, `OpeningTime` (see Moving world models and volumes) |
| Volumes | `Water`, `CorrosiveFluid`, `FreezingWater` (liquids), `VolumeBrush`, `PoisonGas`, `Electricity`, `Burn`, `ColdAir`, `EndlessFall`, `Ice`, `Wind`, `Filter`, `SafteyNet` (spelled so), `Ladder`, `Weather`, `Gravity`, `DamageVolume`, `WorldVolume`, `JumpVolume`, each tied to the world model of the same `Name` (see Moving world models and volumes) |
| Pickups | `WeaponItem` (`WeaponType` string), `FlagItem` (`Teams`, `GameType`, `Filename` model path), and `ObjectSpawnPoint` (`GameType`, `Spawn`), whose role is an open question |
| Sky | `DemoSkyWorldModel` (see Sky) |
| World settings | `WorldProperties`: far clip (`FarZ`), fog (`FogEnable`, `FogColor`, `FogNearZ`, `FogFarZ`), sky fog, volumetric fog, panning sky, wind, `LightmapTFactor` (colour) and more |
| Game-mode controller | `TDMGameControl` |

- Static lights for model lighting: when loading, the engine collects every object of class `Light`, `DirLight` or `ObjectLight` from the object list into a list of static lights, but only those that light objects **and** do not use the light grid (`LightObjects` true and `FastLightObjects` false). For these it computes:

```math
\text{colour} = \text{LightColor} \times \text{BrightScale} \times \text{ObjectBrightScale},\qquad
a_i = \text{AttCoefs}_i \cdot \text{LightRadius}^{\text{AttExps}_i}\ (i = x, y, z),\qquad
\cos\theta_{\text{fov}} = \cos\!\left(\text{FOV} \cdot \frac{\pi}{360}\right)
```

  The defaults are AttCoefs (1, 0, 19), AttExps (0, −1, −2), FOV −1 (no cone), BrightScale 1 and ObjectBrightScale 1. A `LightGroup` name becomes a 32-bit id by the hash below.
- **Name hash** (used for light-group ids and render world-model lookup): start with h = 0; for each byte b of the name, in order, h = (31·h + b) mod 2^32. Case-sensitive.

## Blind object data

At header offset 1. These are opaque chunks that objects fetch by index.

| Size | Type | Meaning |
|---|---|---|
| 4 | u32 | Chunk count |
| … | chunk × count | Chunks |

Chunk: u32 size B, u32 id, then B bytes. An object asks for chunk i with an id, and the engine refuses if the ids differ.

Ids seen: 0x73F53A84 (scatter volumes, for example grass and debris placement) and 0x6AAF0884 (keyframer key data). Their inner layouts belong to those object specs. 20 of the 33 maps have no chunks.

## Light grid

At header offset 2. This is a coarse 3D grid of baked RGB light that the engine samples to light models.

| Offset (relative) | Size | Type | Meaning |
|---|---|---|---|
| 0 | 12 | vec3 | Grid origin (world position of sample 0, 0, 0) |
| 12 | 12 | vec3 | Cell size per axis (250, 250, 250 in every observed file) |
| 24 | 12 | i32 × 3 | X, Y, Z sample counts |
| 36 | 4 | u32 | C, compressed byte count |
| 40 | C | bytes | RLE data decoding to X·Y·Z·3 bytes |

- **Sample order:** the sample at (i, j, k) is at byte 3·(k·X·Y + j·X + i) of the decoded data, as R, G, B bytes. Its world position is origin + (i·cx, j·cy, k·cz).
- **RLE:** repeat until the output is full. Read a tag byte t and let n = (t & 0x7F) + 1. If t & 0x80 is set, read one RGB triple and output it n times. Otherwise, copy the next n RGB triples. The writer never lets a run or span cross the end of an X row. In every observed file the decoded length is exact and the data takes exactly C bytes.
- **Lookup (engine):** for a world point w,

```math
g = (w - \text{origin}) \oslash \text{cell},\qquad i_0 = \operatorname{clamp}(\lfloor g \rfloor_{\to 0}, -1, \text{count}-1)\ \text{per axis}
```

  The next index along an axis is i₀+1, except when i₀ is −1 or the last index; then it is i₀ itself, and i₀ is raised to at least 0. The 8 corners are blended trilinearly using the fractional parts g − ⌊g⌋. Light-group grid samples are added on top (Render data).

## Collision blockers and particle blockers

Header offsets 3 and 4. Both blocks have the same layout:

| Size | Type | Meaning |
|---|---|---|
| 4 | u32 | Polygon count |
| … | polygon × count | Polygons |
| 4 | u32 | Trailer, always 0 |

Polygon: plane (16 bytes), u32 vertex count v, then v vec3 vertices. Vertex order follows the same rule as BSP polygons ((v1 − v0) × (v2 − v0) along n).

- **Trailer** (Open question 17): a field reserved for future expansion. The engine reads it after the polygons and checks it is 0 only in debug builds; a release build ignores its value. It holds nothing. A reader should read it and ignore its value (0 in every observed file).
- **Collision blockers** are invisible polygons that block player movement only: the "player blocker" brushes, which the compiler removes from the BSPs. On load, the engine snaps any normal component with magnitude below 0.0001 to 0 and renormalises. The engine consults them only when moving an object in the player collision mode (the players' cylinder mover). Ray queries (bullets, line of sight, explosion cover), grenades and other point or box movers never see them (Open question 5).
- **Particle blockers** are invisible polygons that stop particles, for example keeping snow and rain out of covered areas.
- Observed counts: blockers 0–698 per map; particle blockers present in 5 of 33 maps.

## Render data

At header offset 5, to the end of the file:

| Part | Layout |
|---|---|
| Main render world | render world (below) |
| World light groups | u32 count, then that many world light-group records (below) |

**Render world:**

| Size | Type | Meaning |
|---|---|---|
| 4 | u32 | B, render block count |
| … | render block × B | Render blocks, in index order |
| 4 | u32 | W, world-model count |
| … | W × (lstring name, render world) | Per world model: its name and a nested render world |

- The nested render worlds never contain further world models (their W is 0). In every observed file, the main render world's W equals M − 1, and the names are exactly the BSP world-model names in file order with the physics BSP left out.
- The engine finds a world model's render data by the name hash of its name. It skips a duplicate hash with a console error.

**Render block** (a node of a binary bounding-box tree; block 0 is the root):

| Size | Type | Meaning |
|---|---|---|
| 12 | vec3 | Box centre |
| 12 | vec3 | Box half-size |
| 4 | u32 | Section count |
| … | section × count | Sections (below) |
| 4 | u32 | V, vertex count (at most 65535) |
| 68·V | vertex × V | Vertices (below) |
| 4 | u32 | F, triangle count |
| 16·F | triangle × F | u32 vertex index a, b, c, then u32 polygon index |
| 4 | u32 | Sky-portal count |
| … | geometry polygon × count | Sky portals |
| 4 | u32 | Occluder count |
| … | (geometry polygon, u32 id) × count | Occluders |
| 4 | u32 | Light-group count |
| … | block light group × count | Block light groups (below) |
| 1 | u8 | Child flags: bit 0 = child 0 present, bit 1 = child 1 present |
| 8 | u32 × 2 | Child block indices into this render world's block array (0 when absent) |

- **Triangles** are grouped by section: section 0 owns the first t₀ triangles, section 1 the next t₁, and so on. The section triangle counts sum to F. Vertex indices are below V; the engine stores them as 16-bit. The polygon index names the BSP polygon the triangle came from: in the main render world it indexes the physics BSP, and in a world model's render world it indexes the BSP with the same name. The packer never shares vertices between sections.
- **Geometry polygon:** u8 vertex count v, v vec3 vertices, then a plane (16 bytes).
- **Render tree:** each block's box contains its children's. The engine walks it from block 0, testing boxes against the view frustum. Observed child flags are 0 (leaf) or 3 (both children).

**Section:**

| Size | Type | Meaning |
|---|---|---|
| 2+n | lstring | Texture 0 name (texture file path; empty for none) |
| 2+n | lstring | Texture 1 name (second texture for dual-texture shaders; usually empty) |
| 1 | u8 | Shader code |
| 4 | u32 | Triangle count (never 0) |
| 2+n | lstring | Texture-effect script name (e.g. a `.tfg` panning script; usually empty) |
| 4 | u32 | Lightmap width |
| 4 | u32 | Lightmap height |
| 4 | u32 | Lightmap byte count Z |
| Z | bytes | RLE lightmap (see Lightmaps) |

- A texture name ending in `.spr` (any case) names a sprite (an animated texture sequence), not a texture.

Shader codes:

| Code | Meaning | Vertex UV use | Seen |
|---|---|---|---|
| 0 | None (not drawn) | – | no |
| 1 | Textured, vertex lit (Gouraud) | UV0 = texture | yes |
| 2 | Lightmap pass | UV0 = lightmap; texture 0 holds a light-animation name such as `LightAnim_BASE`, not a texture | yes |
| 4 | Texture pass of a lightmapped surface | UV0 = texture | yes |
| 5 | Panning sky | – | no |
| 6 | Sky portal (compiler only; sky portals go to their own list) | – | no |
| 7 | Occluder (compiler only) | – | no |
| 8 | Dual texture, vertex lit | UV0 = texture 0, UV1 = texture 1 | yes |
| 9 | Texture pass of a lightmapped dual-texture surface | UV0, UV1 | yes |
| 10 | Render-block splitter (compiler only) | – | no |
| 11 | Unknown (drawn as an error marker) | – | no |

- The engine treats any code other than 1, 2, 4, 8 or 9 as invalid (Open question 17). An invalid section is still read completely (its textures, lightmap and triangle count; its triangles keep their place in the block's list), raises no error, and is **never drawn**. No other codes exist: the compiler's codes stop at 11, and the renderer's own list labels 10 as "unknown" (where the compiler has the render-block splitter) without giving it a shader. A reader should keep such sections and skip them when drawing. Only 1, 2, 4, 8 and 9 occur in the 65 maps of the older regional client.

**Vertex (68 bytes):**

| Offset | Size | Type | Meaning |
|---|---|---|---|
| 0 | 12 | vec3 | Position (world space) |
| 12 | 4 | f32 | U0 |
| 16 | 4 | f32 | V0 |
| 20 | 4 | f32 | U1 |
| 24 | 4 | f32 | V1 |
| 28 | 4 | u32 | Colour as A·2^24 + R·2^16 + G·2^8 + B, so the bytes are B, G, R, A. **Per vertex** (Open question 10): each vertex has its own; in the older regional client's maps about three quarters of the vertex-lit sections have more than one distinct colour. R, G, B are the baked vertex lighting used by shaders 1 and 8; A is a per-vertex blend weight used by the dual-texture shaders 8 and 9 (see Second textures) and is 255 elsewhere. Shaders 2, 4 and 9 do not use R, G, B. |
| 32 | 12 | vec3 | Normal (unit length) |
| 44 | 12 | vec3 | Tangent (unit, along the texture U direction projected into the surface) |
| 56 | 12 | vec3 | Binormal (unit, along the texture V direction projected into the surface) |

- **Texture coordinates** are in texture repeats: 1.0 is one full tile of the texture, independent of its pixel size. Values far outside 0..1 are normal (tens of thousands are seen) and wrap. The compiler computes them from the surface's texture mapping as

```math
u = \frac{P \cdot (x - O)}{w_{\text{tex}}},\qquad v = \frac{Q \cdot (x - O)}{h_{\text{tex}}}
```

  where O, P and Q are the brush's texture origin and axes and w_tex, h_tex the texture size. The file stores only the result.
- Unused UV pairs are 0.

**Block light group** (a switchable light's contribution to one block):

| Size | Type | Meaning |
|---|---|---|
| 2+n | lstring | Light-group name (its id is the name hash) |
| 12 | vec3 | Colour, 0–1 per channel: the compile-time colour, used only until the game sets the group's colour (see Lightmaps, Light groups) |
| 4 | u32 | I, intensity stream byte count |
| I | bytes | Vertex intensity stream |
| 4 | u32 | Section count (equals the block's section count) |
| … | per section: u32 count s, then s sub-lightmaps | Lightmap fix-ups per section |

- **Vertex intensity stream:** one entry per block vertex, in vertex order. Read a byte x. If x ≠ 0, it is that vertex's intensity. If x = 0, read the next byte n: that vertex and the n following vertices all have intensity 0 (1 + n zeros, n up to 255). The entries decode to exactly V vertices. When the group is on, a vertex's colour channel becomes min(255, channel + ⌊colour × x⌋), using the group colour per channel.
- **Sub-lightmap:**

| Size | Type | Meaning |
|---|---|---|
| 4 | u32 | Left (texel x) in the section's lightmap |
| 4 | u32 | Top (texel y) |
| 4 | u32 | Width w |
| 4 | u32 | Height h |
| 4 | u32 | Data byte count |
| … | bytes | Intensity run data decoding to w·h bytes, row by row |

  Run data: read a byte. If it is 0xFF, the next two bytes are n and x, meaning n + 1 texels of intensity x. Otherwise the byte is one texel's intensity. Each texel inside the rectangle gets min(255, channel + ⌊colour × x⌋) added to the section's base lightmap. In the measured maps every rectangle lies inside its section's lightmap, and every block light group sits on lightmap (code 2) sections.

**World light group** (after the main render world):

| Size | Type | Meaning |
|---|---|---|
| 2+n | lstring | Light-group name |
| 12 | vec3 | Colour, 0–1 per channel |
| 12 | u32 × 3 | Offset (x, y, z) inside the light grid, in samples |
| 12 | u32 × 3 | Size (x, y, z) in samples |
| x·y·z | u8 | Intensities, index k·sx·sy + j·sx + i |

- When the group is on, the engine adds colour × intensity to the light-grid samples it covers. A group whose colour is below 1/255 in any channel counts as off.

## Lightmaps

- **Where:** only in shader-2 sections of render blocks, one lightmap per section. A "lightmap count" is the number of shader-2 sections. Every shader-2 section in the measured maps has data, and no other section does.
- **Size:** width and height are each a power of two from 16 to 256 (all 25 combinations occur). The engine creates a texture of the next power of two of each dimension, which is the same size.
- **Pixel format and compression:** 24-bit R, G, B bytes in that order, row by row from the top-left texel, compressed with the same RLE as the light grid (tag byte; high bit = run of one triple, otherwise a span of triples; n = low 7 bits + 1). It decodes to exactly width·height·3 bytes using exactly Z input bytes. There is no other compression.
- **Mapping:** the vertices of a shader-2 section carry the lightmap coordinates in UV0, normalised to the section's lightmap: u = 0 is the left edge of texel column 0 and u = 1 the right edge of column width − 1 (V likewise with rows). In WAREHOUSE all values lie within 0.002..0.998. Across all plaintext maps the minimum is 0.00195 and a few values exceed 1 (up to 3.02); in the older regional client 21 sections in three maps have them. **Clamping** (Open question 9): the coordinates are interpolated across the triangle unchanged and the lightmap stage samples with clamp addressing on both axes, per pixel: any pixel whose interpolated coordinate is above 1 (or below 0) reads the edge texel row or column (with the usual bilinear filtering inside the texture). It is not wrapped, and the vertices are not clamped. The compiler packs each polygon's lightmap into a rectangle of the page with a one-texel border copied from the polygon's edge texels (corner texels averaged), so bilinear filtering does not bleed. The page's own outer border is removed before storage, and the engine relies on texture clamping there. Polygon lightmap texels were laid out on the grid `lmgridsize` (world units per texel) at compile time.
- **How it is drawn** (Open question 7): a lightmapped polygon appears twice, once in a shader-2 section (lightmap) and once in a shader-4 or shader-9 section (texture), and the two are multiplied:

```math
\text{out} = s\cdot\big(\text{tex}\cdot T\big)\cdot\big(\text{lm}\cdot T\big),\qquad s = 2\ (\text{console variable } \texttt{Saturate} = 1,\ \text{the default}),\ \ s = 1\ (\texttt{Saturate} = 0)
```

  with all colours in 0..1 per channel. T is the extra lightmap scale: the world properties' `LightmapTFactor` colour divided by 255 per channel (the game hands it to the renderer as a console colour; with none set it is white). It multiplies both passes in the reference's renderer. Every one of the 65 maps of the older regional client stores (255, 255, 255), so T = 1 in stock maps and the stock result is 2·tex·lm. In the reference renderer (non-deferred path) the texture pass is drawn first with ordinary depth, then the lightmap pass is drawn over it with an equal-depth test and blended as dest × src × 2 (dest × src when `Saturate` is off), with the fog colour set to mid-grey (0x808080; white without `Saturate`) for that pass so fog survives the multiply. The older path of the same renderer drew the lightmap first and multiplied the texture onto it; the product is the same. A console variable that switches the multiply to 2× regardless (`FullBright`) is off by default. Light groups modify the decoded lightmap before upload (Light groups below).
- **Vertex-lit sections** (shaders 1 and 8) are drawn as tex × vertex colour × s (the same `Saturate` doubling) × T (Open question 10).
- **Light groups** (Open question 12): the switchable lights are the `LightGroup` objects. Their baked contribution is stored three ways: block vertex intensities, section sub-lightmaps and light-grid fix-ups, all keyed by the light-group name. 7 of the 33 Steam maps use them; 19 of the older regional client's 65.
  - **State at load.** Every group starts at the colour stored with it in the render data. The game's `LightGroup` object of the same name then sets the group, as soon as the client learns of the object (during the map load), to `StartColor`/255 if `StartOn` is true and to black (0, 0, 0) if it is false. Its defaults are `StartOn` true and `StartColor` (255, 255, 255); every observed group has `StartColor` (255, 255, 255), and `StartOn` varies (for example both groups of SNOWSTORM start off).
  - **Switching.** Map script messages to the `LightGroup` object by its name: `ON`, `OFF`, `TOGGLE`, and `COLOR v` (all channels v/255) or `COLOR r g b` (per channel /255; a negative value keeps that channel). `COLOR` changes the colour without switching the group on. After each message the object sends all clients the group's effective colour: its colour if on, black if off. Observed senders are command triggers, triggers and door commands (for example a fridge door's opening command switches its light group on).
  - **Applying a colour.** For every render block that has the group, its stored colour is replaced and the block's lighting is rebuilt: each affected lightmap is decoded afresh from the file and every group's sub-lightmaps are added with the groups' **current** colours, so switching a group off restores the baked texels exactly; vertex colours are rebuilt from the stored ones the same way. The addition per channel is min(255, base + ⌊colour × x⌋) (Render data). A group whose colour × 255 truncates to 0 in all three channels counts as off and adds nothing. The light-grid fix-ups (world light groups) follow the same colour.
- An engine variant can replace a section's lightmap with an external high-resolution texture found by world name, block index and section index. No such files exist in the Steam install's game folder, and a reader can ignore this.

## What is drawn and what is collision only

- **Drawn:** only the render data. The BSPs are never drawn.
- **Collision and queries:** the physics BSP (the static level), the ordinary world-model BSPs (moved with their objects), and the collision blockers (players only). Ray hits report a BSP polygon, whose surface gives flags and surface type.
- BSP polygons that the main render world never references in the measured maps are the sky-portal surfaces (flag bit 4) and invisible surfaces (bit 2), plus a handful of others. In 3,863 of the 3,895 ordinary world models, every polygon is referenced by that model's render data.
- The compiler's shader choice per polygon, in priority order:
  1. visibility blocker → occluder list;
  2. render-block splitter → none;
  3. sky portal → sky-portal list;
  4. invisible → not drawn;
  5. lightmapped → lightmap pass plus texture pass;
  6. two textures → dual texture;
  7. Gouraud, lightmapped or flat shaded → vertex lit;
  8. otherwise → unknown.
- **Which polygons collide** (Open question 5). The BSP holds solid and non-solid polygons alike (in the older regional client's maps about 28 % of physics-BSP polygons lack the solid bit: trees, foliage, decoration and the like; WAREHOUSE has 4,960 of 37,648). The engine's queries treat them differently:
  - **Player and character movement** (the cylinder mover, and the box mover) collides only with polygons whose surface has the **solid bit**. The invisible bit plays no part: an invisible solid polygon (a clip wall) blocks players.
  - **Rays** (bullet traces, line-of-sight and explosion cover traces, every segment query) test **every** polygon of the BSP; the solid bit is not looked at. A ray query may pass a polygon filter; the bullet and cover filters skip only surface **types** that the game's surface table marks invisible, and no stock surface type is so marked, so in practice rays stop at non-solid and invisible polygons too. What a bullet then does at the surface (penetration, shoot-through) is the weapon spec's business.
  - **Point movers** (grenades and other projectiles) test every polygon too, except that they may ask to skip polygons with the **invisible bit**, which grenades do (`grenades.md`, section 5).
  - **Collision blockers** stop only the player mover (Collision blockers above); bullets, grenades and cover traces pass through them.
  - Solid world models (doors, platforms) are collided the same ways using their own BSPs; non-solid ones are ignored by movement but still hit by rays if they have the ray-hit flag (Moving world models and volumes).

## Sky

- **Sky portals:** each render block lists sky-portal polygons. Through these the engine draws the sky scene instead of world geometry. In every measured map there are exactly six, the six faces of the world-extents box with normals pointing inward (for example normal (1, 0, 0) with d = −extent).
- **Sky scene:** objects of class `DemoSkyWorldModel` (one per measured map) name a world model by their `Name`, for example `SkyPointerBox00`. That world model's geometry, placed somewhere out of view in the map, is rendered as the sky. `SkyDims` (vector) is the size of the sky box, and `InnerPercentX/Y/Z` and `Index` order and scale it. `WorldProperties` adds `AllSkyPortals` (boolean), sky fog and the panning-sky settings (`PanSky`, `PanSkyTexture`, offsets and scales).
- **How the sky is drawn** (Open question 11). Each frame, before the world's sections are drawn:
  1. The screen rectangle covered by the visible blocks' sky portals is found (the bounding rectangle of the portals' projected vertices, clipped to the view). If it is under 1 pixel in width or height, or the map has no sky objects, there is no sky this frame.
  2. A sky camera is set up with the main camera's rotation and field of view, limited to that rectangle, near plane 0.01 u and far plane the `SkyFarZ` console value (default 10000 u), scaled by the `SkyScale` console value (default 1). Its position follows the player through a small inner box: with the sky definition's view box V = sky position ± `SkyDims`·`InnerPercent` (per axis) and the world extents E,

```math
\vec c_{sky} = \vec V_{min} + (\vec V_{max} - \vec V_{min}) \odot \frac{\vec p_{cam} - \vec E_{min}}{\vec E_{max} - \vec E_{min}}
```

   With the usual `SkyDims` 256 and `InnerPercent` 0.001 the sky camera moves at most ±0.256 u (no visible parallax); with 0.1, ±25.6 u.
  3. Every sky object is drawn in slot order (see `Index` below) with depth testing and depth writing **off**: the sky world model's render data (and any sky sprites or polygon grids). Fog stays in whatever state the world uses, but with the start and end distances `SkyFogNearZ` / `SkyFogFarZ`; a sky world model with its fog-disable flag draws without fog. `SkyFogEnable` is read by no drawing code.
  4. The depth buffer is cleared, and the world is then drawn normally over the sky. Sky-portal polygons themselves are never drawn; world geometry in front of the sky simply covers it.
  
  Each `DemoSkyWorldModel` sets the sky definition (its position, `SkyDims` and inner box) when it is created and puts itself in sky slot `Index`; an object put in an occupied slot replaces the one there. Every observed object uses slot 0, so in a map with two (for example HEAVYMETAL's `SkyPointerBox` and `SkypointerBox_Q`) the last one created is the sky and sets the definition; an `ON` message to an object puts it back in its slot with its own definition. The sky world model is drawn at its own position in its own space; it is not moved next to the player.
- **World properties and fog** (Open question 11). `WorldProperties` is one object per map; its values become console values for the renderer. Fog is linear in view depth z (table fog):

```math
f = \operatorname{clamp}\!\left(\frac{\texttt{FogFarZ} - z}{\texttt{FogFarZ} - \texttt{FogNearZ}},\,0,\,1\right),\qquad \text{colour} = f\cdot\text{colour} + (1-f)\cdot\frac{\texttt{FogColor}}{255}
```

  | Property | Type | Default | Meaning |
  |---|---|---|---|
  | `FogEnable` | boolean | true | Fog on |
  | `FogColor` | colour | (127, 127, 127) | Fog colour, 0–255 |
  | `FogNearZ`, `FogFarZ` | real | 1, 5000 | Fog start and end depth, u |
  | `SkyFogNearZ`, `SkyFogFarZ` | real | 100, 1000 | The same for the sky pass |
  | `SkyFogEnable` | boolean | false | Stored, not used for drawing |
  | `FarZ` | real | (map value) | Far clip distance, u |
  | `ClampFarZ` | boolean | true | If true: FarZ becomes 2·`FogFarZ` when fog is on and that is smaller, and FarZ is capped at the world box's diagonal |
  | `AllSkyPortals` | boolean | | Renderer option (all sky portals) |
  | `PanSky`, `PanSkyTexture`, `PanSkyOffsetX/Z`, `PanSkyScaleX/Z` | | off, 10 | Panning sky layer (off in every observed map) |
  | `VFog`, `VFogMinY`, `VFogMaxY`, `VFogMinYVal`, `VFogMaxYVal`, `VFogDensity`, `VFogMax` | | off, 0, 1300, 0.5, 0, 1800, 120 | Volumetric (height) fog, on in one observed map |
  | `LightmapTFactor` | colour | (255, 255, 255) | Extra lightmap scale (Lightmaps) |
  | `EnvironmentMap` | string | | Default environment map texture |
  | `GlobalSoundFilter` | string | `UnFiltered` | Sound filter for the map |
  | `Wind` | vector | (0, 0, 0) | Wind for effects |
  | `DrawAllModelShadows`, `ShadowMinColorComponent`, `ShadowMaxProjDist`, `ShadowAlpha` | real | 0, 40, 200, 1 | Model shadow settings |
  | `LMAnimStatic` | boolean | false | Light-animation option |
  | `AIMaxNumber` | long | 30 | Bot cap |
  | `MusicControlFile`, `MinMusicMood` | string | | Music |
  
  One observed map also stores a group of later renderer settings (bloom, shadow maps, SSAO, HDR exposure, volumetric fog, sun). They belong to the reference renderer's deferred path, which switches itself off when its shader archive is missing; the Steam install has no such archive, so the forward path described here is the one in use.

## Second textures

How sections with more than one texture source are combined (Open question 13). All colours 0..1; s is the `Saturate` doubling (2 by default) and T the lightmap factor (Lightmaps). Texture coordinates wrap.

- **Dual texture** (shaders 8 and 9; texture 0 with UV0, texture 1 with UV1). The blend weight is a = vertex alpha × texture-0 alpha:

```math
\text{base} = a\cdot\text{tex}_0 + (1-a)\cdot\text{tex}_1,\qquad \alpha_{out} = a
```

  If texture 1 has the fullbright flag (`textures_dtx.md`) the second texture is added instead: base = tex₀ + a·tex₁. Shader 8 then multiplies by the vertex colour and s; shader 9 is the texture pass of a lightmapped surface and is multiplied by the lightmap pass like shader 4. Map makers use the per-vertex alpha to paint between two ground textures (in DESERTHOLE 167 of the 186 dual sections have varying vertex alpha, 15 are all 255 and 4 all 0).
- **Detail texture** (the base texture names one, `textures_dtx.md`). The detail texture is sampled at coordinates derived from the section's UV0 by the base texture's detail scale and rotation, and combined with the base colour as base + detail − 0.5 (clamped; console `DetailTextureAdd` 1, the default) or base × detail (`DetailTextureAdd` 0); alpha from the base. The result then goes through the normal lighting (lightmap pass, or vertex colour). Off when the `DetailTextures` console value is 0.
- **Environment map** (`envmap` / `envmapalpha` link). The second stage samples the environment map with coordinates generated from the camera-space reflection vector (for a flat map: transformed by the world environment-map matrix, then scaled by −0.5/`EnvScale` and offset by 0.5; a cube map uses the vector directly) and combines it as base + env − 0.5 (`EnvMapAdd` 1, the default) or base × env; the alpha variant weights it by the base texture's alpha. Off when `EnvMapEnable` is 0.
- **Fullbright mask** (texture flag, `textures_dtx.md`). In the reference renderer the section is drawn a second time right after its texture pass, blended by the texture's own alpha. In the renderer's older path the texture pass came after the lightmap, so this second draw puts the unlit texture back where alpha is high (out = tex·a + lit·(1 − a)). In the path the reference build uses (texture first, lightmap multiplied last, Lightmaps) the second draw lands on the unlit texture and the lightmap still multiplies over it afterwards, so it has no visible effect. Which applies to the Steam build is open (Open question 13).
- Only one linked texture per base texture is used (detail **or** environment map), as `textures_dtx.md` describes.

## Moving world models and volumes

(Open question 14.) These are game objects, listed here because each is tied to a BSP world model of the same `Name`; their motion and effects belong to gameplay specs.

**World models.** Every world-model object has the boolean properties `Solid` and `Rayhit` (read by the engine's base object), which set its solid and ray-hit flags:

- **Solid** world models block player movement (the mover collides with their BSP; a moving one pushes or is stopped by what it hits), are collided like the level by grenades and other point movers, and block explosion cover traces. This holds at every point of their motion.
- **Rays** (bullets) test world models that are solid **or** ray-hit, against their BSP in its current position. A non-solid, ray-hit world model (184 `WorldModel`s and 28 `RotatingDoor`s in the older regional client's maps) stops bullets but not players.
- A world model with neither flag is visual only.

| Class | Moves by | Key properties (defaults) |
|---|---|---|
| `SlidingDoor`, `SlidingWorldModel` | Translation | `MoveDir` (vector), `MoveDist` (real, u) |
| `RotatingDoor`, `RotatingWorldModel` | Rotation about a point | `RotationAngles` (vector), `RotationPoint` (name of the object whose position is the pivot), `RotateAway` |
| `SpinningWorldModel` | Continuous rotation | `RotationAngles` |
| `WorldModel` | Nothing by itself; can be keyframed (`IsKeyframed`, moved by a `KeyFramer` object) or hidden | |
| `Breakable` | Shakes, then falls or vanishes when broken | `MoveDir`, `MoveDist`, hit points |

The moving classes share: `PowerOnTime` (1 s; doors also `OpeningTime` / `ClosingTime`), `PowerOffTime` (0), `MoveDelay` (0), `OnWaitTime` / `OffWaitTime` (0; doors `OpenWaitTime` / `CloseWaitTime`), `StartOn` / `StartOpen`, `TriggerOff` / `TriggerClose`, `RemainOn` / `RemainOpen`, `PlayerActivate`, `Locked`, `DoorLink`, and `Waveform`: with r the move's linear speed and p the fraction of the move done, the speed is r (`Linear`), (π/2)·r·sin(πp) (`Sine`), (π/2)·r·cos(πp/2) − r/1000 (`SlowOff`) or (π/2)·r·sin(πp/2) + r/1000 (`SlowOn`). Observed in the older regional client's 65 maps: 1,324 `WorldModel`s with neither flag, 677 solid and ray-hit, 184 ray-hit only and 90 solid only; 712 solid breakables; 257 solid rotating and 273 solid sliding doors; mostly `Linear` (some `Sine`, `SlowOn`, `SlowOff`).

**Volumes.** Volume classes are containers: the engine calls them every physics step for each object inside their BSP. Each has `Damage` (per second), `DamageType`, `Viscosity`, `Current`, `Friction`, `PlayerSpeedFactor`, fog and tint, `SoundFilter`, `SurfaceOverride`, `Hidden` (switched by `ON` / `OFF`) and `RayHit` (default false). Damage is dealt to players inside as over-time damage of `Damage` × frame time per frame. Viscosity and current act on non-player objects as in `grenades.md` section 7; players' volume movement is the movement spec's.

| Class | What it does (class defaults) |
|---|---|
| `Water`, `CorrosiveFluid`, `FreezingWater` | Liquids: liquid gravity −200 u/s² for non-players, viscosity 0.035, underwater sound filter; corrosive deals 5 burn/s, freezing 10 freeze/s |
| `VolumeBrush` | Generic volume with all the properties above (default damage type choke) |
| `PoisonGas` | 5 poison/s |
| `Electricity` | 20 electrocute/s |
| `Burn` | 20 burn/s |
| `ColdAir` | 5 freeze/s |
| `EndlessFall` | Kill volume: 10⁸ "endless fall" damage per second. `PendingSec` (real, default 0): if above 0, a player must have been inside for that long (checked every 0.5 s) before the damage applies |
| `Ice` | Friction 0 |
| `Wind` | Friction 1, no damage |
| `Filter` | Sound filter only |
| `SafteyNet` | No damage, no physics; a marker for game code |
| `Ladder` | Ladder for characters (viscosity 0.4) |
| `Weather` | Rain or snow effect inside, with its own fog and view distance |
| `Gravity` | `PlayerGravity` (default −2000) for players inside |
| `DamageVolume` | Damage restricted to `TargetTeam`, can be `Locked` |
| `WorldVolume`, `JumpVolume` | Mode volumes (`JumpVolume` launches players: `Velocity`) |

Property defaults and meanings shared by all volume classes (players' use of them: `movement.md`, "Volumes"):

- `Friction` (real, default 1 in every class, `Ice` 0) and `Viscosity` (real, default 0; liquids 0.035, `Ladder` 0.4) are clamped to [0, 1] on load; `PlayerSpeedFactor` (real, default 1 in every class) is clamped to at least 0. `Current` is a vector in u/s, default (0, 0, 0).
- `DamageType` (string) names an entry of the game's damage-type table, matched without regard to case; a name not in the table gives an invalid type. Class defaults: `VolumeBrush` `CHOKE`; `CorrosiveFluid` and `Burn` `BURN`; `FreezingWater` and `ColdAir` `FREEZE`; `PoisonGas` `POISON`; `Electricity` `ELECTROCUTE`; `EndlessFall` `ENDLESS FALL`; `DamageVolume` `FAK_HEAL` (a heal); the others `UNSPECIFIED`. Seen in the older regional client's maps: `CHOKE`, `BURN`, `FREEZE`, `POISON`, `ENDLESS FALL`, `FAK_HEAL`, `UNSPECIFIED`.
- `JumpVolume`: `Velocity` (real, default 1000) is a speed in u/s, and a `VELOCITY <speed>` message changes it. The launch direction is the forward vector of the object's `Rotation` (Objects, "Rotation to matrix"); the object itself is then given no rotation. Observed: the gravity map's 7 pads point straight up (forward within 1° of +Y; six at 800, one at 1400), the dam map's 3 pads (2000, 2000, 3000) have forward (0, −0.707, −0.707) (`movement.md` Open question 29).
- `Gravity`: `PlayerGravity` (real, u/s², default −2000) replaces the player's gravity; overlapping volumes add up. Observed −300 in both gravity maps.

## Map identity

(Open question 15.) Neither the engine nor the game checks a map file's hash or size between server and client. The server sends each client its list of used files (an id, the file size and the file name per entry); the client looks each name up in its own mounted archives, ignores the size, and treats every entry as present even when it cannot find it (it only prints a console line). The world to load is then named by its file id, and the client loads its **own** copy of that name; a copy that fails to parse fails the load with an invalid-world error. The game's own start-up file check covers program files and the attribute archive, not the world archives. So two different maps with the same archive path are not told apart.

## Other contents

- **Terrain:** none. Terrain depth and BSP section counts are always 0; a non-zero section count is rejected.
- **Portals and sectors:** none. User portals must be 0, and there are no leaves or visibility lists; visibility uses the render-block tree and the occluders.
- **Occluders:** geometry polygons with a u32 id, from visibility-blocker surfaces. Present in 10 of 33 maps. The engine uses them for occlusion culling. They are never drawn.
- Nothing follows the world light groups; the file ends there.

## Coordinate conversion

The file frame is left-handed: +X right, +Y up, +Z forward. To get a right-handed Y-up frame, mirror Z:

```math
p' = (x,\ y,\ -z),\qquad n' = (n_x,\ n_y,\ -n_z),\qquad d' = d
```

- Apply it to every position, vertex, normal, tangent and binormal, box centre and box corner (min and max swap their z), plane, and the extents.
- Mirroring reverses handedness. To keep "(v1 − v0) × (v2 − v0) along the normal" (and the front faces), **reverse the vertex order** of every polygon and swap two indices of every triangle.
- Rotations: with S = diag(1, 1, −1), the converted rotation matrix is R' = S·R·S. For a yaw-only rotation this gives a facing of (sin yaw, 0, −cos yaw); yaw 0 now faces −Z, the usual right-handed forward.
- Texture and lightmap UVs do not change.
- Units are not changed by this conversion.

## Edge cases

- **Leaves:** if a leaf count were non-zero, each leaf is a u16 list count c. If c is 0xFFFF, one u16 follows (the leaf shares another leaf's lists). Otherwise c entries follow, each a u16 portal id, a u16 byte length and that many bytes. The engine skips all of it. Observed files have none.
- **Texture-name truncation:** names longer than the engine's buffers (63 characters for world-model names, 260 for section texture names, 255 for class and property names) are cut on load. A reader should keep the full string.
- **Empty info string:** L = 0 is valid (3 of the 33 maps: BOSSBATTLE_CP, 16ROOMS, SNOWSTORM).
- **Empty blocks:** zero objects, zero blind chunks, zero blockers (the 0 trailer is still present), zero render blocks and zero world light groups are all valid layouts.
- **World-model record prefix:** the leading u32 is 0 everywhere; ignore its value.
- **Movable world models:** the engine loads the BSP of a movable world model twice (one copy to transform), which has no effect on the file.
- **Vertex index width:** triangle indices are u32 in the file but held as u16 by the engine. The compiler keeps blocks under 65536 vertices and triangles.
- **Absent children:** a cleared child-flag bit means "no child" even though an index (0) is stored.
- **Encrypted maps:** a still-encrypted payload from a V2 archive fails the version check. Decrypt with the archive layer first.

## Quirks

- **Integer properties stored as reals.** Long-integer and flags properties are f32 and are truncated toward zero on load. Keep this: values above 2^24 lose precision, as in the game.
- **Rotation padding.** The fourth float of a rotation property is always 0 and unused.
- **Static-light pass overrun.** The engine's static-light pass reads property data into a 256-byte buffer. For a light property longer than 256 bytes it jumps to the end of the object but keeps reading properties from there. A reader must parse by the stored lengths instead. No light property that long occurs.
- **"FastLightObjects" carry-over.** In the same pass, a light without a `FastLightObjects` property inherits the value of the previous light that had one, starting from true. Keep this only if static lights are rebuilt the engine's way.
- **Sub-lightmap escape.** The compiler writes a lone intensity of 255 as 0xFF, 0, 0, which decodes as one texel of intensity 0. This is a writer bug, kept by any faithful decoder. It never occurs in the measured maps.
- **Surface flag 1** (removed by compiler) is still set on some surviving surfaces; do not drop surfaces because of it.

## Synthetic fixtures

All values are little-endian; f32 values are given as numbers. These files are ours and contain nothing from the game.

### Fixture A: minimal valid file (432 bytes)

One physics BSP holding one triangle, no objects, a 1×1×1 light grid, and no render blocks.

| Offset | Size | Field | Value |
|---|---|---|---|
| 0 | 4 | version | 85 |
| 4 | 4 | object list offset | 352 |
| 8 | 4 | blind data offset | 356 |
| 12 | 4 | light grid offset | 360 |
| 16 | 4 | blocker offset | 404 |
| 20 | 4 | particle blocker offset | 412 |
| 24 | 4 | render data offset | 420 |
| 28 | 32 | reserved | all 0 |
| 60 | 4 | info length | 0 |
| 64 | 24 | extents | min (−64, −64, −64), max (64, 64, 64) |
| 88 | 12 | world offset | (0, 0, 0) |
| 100 | 24 | tree box | min (−64, −64, −64), max (64, 64, 64) |
| 124 | 4 | tree node count | 1 |
| 128 | 4 | terrain depth | 0 |
| 132 | 1 | tree bits | 0x00 |
| 133 | 4 | world model count | 1 |
| 137 | 4 | record prefix | 0 |
| 141 | 4 | BSP flags | 0x14 |
| 145 | 12 | name | u16 10, `PhysicsBSP` |
| 157 | 40 | counts | points 3, planes 1, surfaces 1, user portals 0, polygons 1, leaves 0, poly-vertex refs 3, vis size 0, leaf lists 0, nodes 1 |
| 197 | 36 | box and translation | min (0, 0, 0), max (32, 0, 32), translation (0, 0, 0) |
| 233 | 8 | texture list size, count | 8, 1 |
| 241 | 8 | texture names | `Default` and a 0 byte |
| 249 | 1 | polygon vertex counts | 3 |
| 250 | 16 | plane 0 | normal (0, 1, 0), d 0 |
| 266 | 8 | surface 0 | flags 0x1001 (solid, Gouraud), texture 0, surface type 0 |
| 274 | 20 | polygon 0 | surface 0, plane 0, points 0, 1, 2 |
| 294 | 14 | node 0 | polygon 0, u16 0, back −2, front −1 |
| 308 | 36 | points | (0, 0, 0), (0, 0, 32), (32, 0, 0) |
| 344 | 4 | root node | 0 |
| 348 | 4 | section count | 0 |
| 352 | 4 | object count | 0 |
| 356 | 4 | blind chunk count | 0 |
| 360 | 12 | grid origin | (−64, −64, −64) |
| 372 | 12 | cell size | (250, 250, 250) |
| 384 | 12 | sample counts | 1, 1, 1 |
| 396 | 4 | compressed size | 4 |
| 400 | 4 | RLE data | 0x80, 40, 40, 40 |
| 404 | 8 | blockers | count 0, trailer 0 |
| 412 | 8 | particle blockers | count 0, trailer 0 |
| 420 | 4 | render block count | 0 |
| 424 | 4 | render world-model count | 0 |
| 428 | 4 | world light-group count | 0 |

Expected after parsing: version 85; M = 1; the physics BSP is world model 0 with 1 polygon, (v1 − v0) × (v2 − v0) = (0, 1024, 0), along the plane normal. Point (8, 1, 8) locates to "in" (front of the plane) and (8, −1, 8) to "out". The decoded light grid is one sample (40, 40, 40). The file ends at 432.

### Fixture B: one object of every property type, one render block (916 bytes)

Same as Fixture A up to offset 352, then:

| Offset | Size | Field | Value |
|---|---|---|---|
| 352 | 4 | object count | 1 |
| 356 | 2 | record length R | 175 |
| 358 | 6 | class | u16 4, `Demo` |
| 364 | 4 | property count | 8 |
| 368 | 16 | property `Name` | name u16 4 `Name`, type 0, flags 0, D 3, data u16 1 `A` |
| 384 | 24 | property `Pos` | type 1, flags 0, D 12, (1, 2, 3) |
| 408 | 26 | property `Color` | type 2, flags 0, D 12, (255, 128, 0) |
| 434 | 17 | property `Real` | type 3, flags 0, D 4, 0.5 |
| 451 | 17 | property `Bits` | type 4, flags 0, D 4, 6.0 (reads as integer 6) |
| 468 | 14 | property `Flag` | type 5, flags 0, D 1, 1 |
| 482 | 18 | property `Count` | type 6, flags 0, D 4, 7.0 (reads as integer 7) |
| 500 | 33 | property `Rotation` | type 7, flags 0, D 16, (0, 1.5707964, 0, 0) |
| 533 | 4 | blind chunk count | 0 |
| 537 | 44 | light grid | as Fixture A |
| 581 | 8 | blockers | 0, 0 |
| 589 | 8 | particle blockers | 0, 0 |
| 597 | 4 | render block count | 1 |
| 601 | 24 | block box | centre (16, 0, 16), half-size (16, 1, 16) |
| 625 | 4 | section count | 1 |
| 629 | 30 | section 0 | texture 0 u16 7 `Default`, texture 1 u16 0, shader 1, triangles 1, effect u16 0, lightmap 0 × 0, byte count 0 |
| 659 | 4 | vertex count | 3 |
| 663 | 204 | vertices | positions (0, 0, 0), (0, 0, 32), (32, 0, 0); UV0 = (x/64, z/64); UV1 (0, 0); colour 0xFFFFFFFF; normal (0, 1, 0); tangent (1, 0, 0); binormal (0, 0, 1) |
| 867 | 4 | triangle count | 1 |
| 871 | 16 | triangle 0 | 0, 1, 2, polygon 0 |
| 887 | 12 | sky portals, occluders, light groups | 0, 0, 0 |
| 899 | 1 | child flags | 0 |
| 900 | 8 | children | 0, 0 |
| 908 | 4 | render world-model count | 0 |
| 912 | 4 | world light-group count | 0 |

Header offsets for Fixture B: 352, 533, 537, 581, 589, 597. Expected: one object of class `Demo` with the eight decoded values above, and yaw 1.5707964 facing (1, 0, 0) in file axes. One render block with V = 3 and F = 1, whose triangle refers to physics-BSP polygon 0. The name hash of `A` is 65.

### Fixture C: negative tests

1. Fixture A with the version set to 84 → rejected.
2. Fixture A with the user-portal count set to 1 → rejected.
3. Fixture A with the section count at 348 set to 1 → rejected.
4. Fixture A with polygon 0's point index 2 changed to 3 → rejected (index ≥ P).
5. Fixture A with the tree node count set to 5 and the tree bits left 0x00 → rejected (count mismatch).
6. Fixture A truncated to 431 bytes → rejected.
7. Light-grid RLE that would decode past X·Y·Z·3 bytes → rejected.

## Test cases

Measured on Combat Arms: the Classic, Steam build id 25595979 (the Steam manifest on this PC shows that build installed, with an update to 25901084 pending and not yet applied). Files are named by archive and entry. Reals are as stored; compare floats with a tolerance of 1e-3. "Total polygons" and "total points" sum over all BSP world models, physics BSP included. "Lightmaps" counts shader-2 sections. "Render vertices" and "render triangles" sum over all render blocks, world models included.

1. **Every plaintext map** in WORLDS2 (19 maps) and WORLDS3 (14 maps): version 85, all eight reserved header words 0, each block's sequential parse ends exactly at the next header offset, and the render data plus world light groups end exactly at the file size. Exactly one world model has flags 0x14, it is the last one and named `PhysicsBSP`; all others have flags 0x02. Every world-model record prefix is 0. The main render world's world-model names equal the BSP world-model names in order, without the physics BSP. Every map has exactly 6 sky-portal polygons.

2. **WORLDS2\WAREHOUSE.DAT:**
   - File size 18,076,579; version 85.
   - Header offsets 3,056,494 / 3,303,416 / 3,307,244 / 3,333,968 / 3,337,304 / 3,337,312.
   - Info string (36 bytes) `ambientlight 30 30 34; lmgridsize 10`.
   - Extents min (−9952, −10080, −10112), max (9952, 10080, 10112); world offset (−480, 5984, 1216).
   - World tree: N = 5461, ending at byte 851.
   - World models: 110. Total polygons 38,404; total points 31,134.
   - World model 0: `EndlessFall00`, flags 0x02, 8 points, 6 planes, 1 surface, 6 polygons, 6 nodes, 2 textures.
   - Physics BSP (world model 109): 30,106 points, 33,628 planes, 298 surfaces, 37,648 polygons, 114,984 polygon-vertex references, 90,899 nodes, 269 textures, root node 0.
   - Objects 370. Blind chunks 6, all id 0x73F53A84, sizes 230, 550, 902, 544, 472, 1078.
   - Light grid counts (80, 81, 81), cell 250, compressed 26,684 bytes.
   - Blockers 50; particle blockers 0; world light groups 0.
   - Render: 145 main blocks, 109 render world models, 2,114 sections, 305 lightmaps, 151,153 render vertices, 69,385 render triangles, 0 occluders. Sections per shader: 1 → 817, 2 → 305, 4 → 980, 8 → 12.
   - Root block: centre (0, 0, 0), half-size (9952, 10080, 10112), 50 sections, 3,836 vertices, 1,956 triangles, child flags 3, children 66 and 1. Its first section has texture 0 `LightAnim_BASE`, shader 2, 2 triangles, lightmap 256 × 256 in 7,422 bytes.
   - The highest polygon index in the main render world's triangles is 37,647.
   - Classes: `Light` 70, `ObjectSpawnPoint` 60, `EndlessFall` 55, `WorldModel` 41, `GameStartPoint` 40, `DirLight` 38, `SpecialFX` 24, `Breakable` 12, `TDMGameControl` 2, `StaticSunLight` 1, `DemoSkyWorldModel` 1, `WorldProperties` 1.
   - `GameStartPoint` by (`GameType`, `Teams`): (Team Death Match, JSF) 12, (Team Death Match, Creed) 12, (FreeForAll, `<none>`) 16.
   - Object 92 decodes to class `GameStartPoint` with 13 properties: `Name` = `GameStartPoint11` (D 18); `Pos` = (618, −5546, −2271.0002); `Rotation` = (0, −0.000001, 0, 0); `RenderGroup` = 0; `Template` = false; `GameType` = `Team Death Match`; `Teams` = `JSF` (D 5); `Spawn` = 1; `ModelTemplate` = `Normal`; `PhysicsModel` = `Normal`; `Command` = empty (D 2); `Locked` = false; `SendCommandOnRespawn` = false. Flags of `Pos` = 0x100000; flags of `GameType` = 0x20000.
   - Every `GameStartPoint` position locates to "in" in the physics BSP.

3. **WORLDS3\FREEZER.DAT:**
   - File size 11,343,910; version 85.
   - Header offsets 2,118,069 / 2,255,753 / 2,255,757 / 2,984,626 / 2,988,314 / 2,988,322.
   - Info string (21 bytes) `ambientlight 40 40 40`.
   - Extents ±(32768, 28672, 32768); world offset (0, 0, 0).
   - World tree: N = 21,845, ending at byte 2,884.
   - World models 32. Total polygons 26,198; total points 14,820.
   - Physics BSP (world model 31): 13,704 points, 23,305 planes, 54 surfaces, 24,472 polygons, 65,717 nodes, 48 textures.
   - World model 3: `CMD_A_Radio`, flags 0x02, 8 points, 6 planes, 6 polygons, 6 nodes. The object of that `Name` is a `RotatingDoor` with `Rotation` (0, 3.1415930, 0, 0).
   - Objects 275; blind chunks 0.
   - Light grid counts (263, 230, 263), compressed 728,829 bytes.
   - Blockers 64; particle blockers 0; world light groups 0.
   - Render: 73 main blocks, 31 render world models, 705 sections, 349 lightmaps, 83,969 render vertices, 31,729 render triangles. Sections per shader: 1 → 165, 2 → 349, 4 → 142, 8 → 22, 9 → 27.
   - Root block: 62 sections, 2,613 vertices, 1,122 triangles, child flags 3, children 38 and 1.
   - Classes: `GameStartPoint` 80, `ObjectSpawnPoint` 60, `Light` 25, `SpecialFX` 24, `EndlessFall` 22, `DirLight` 12, `SoundFX` 10, `WorldModel` 6, `TDMGameControl` 5, `FlagItem` 2, `SlidingDoor` 2, `RotatingDoor` 1.
   - `GameStartPoint` by (`GameType`, `Teams`): (Capturetheflag, JSF) 12, (Capturetheflag, Creed) 12, (Team Death Match, JSF) 12, (Team Death Match, Creed) 12, (Quarantine, JSF) 16, (FreeForAll, `<none>`) 16.
   - Object 0 decodes to class `Light`: `Name` `Light11`, `Pos` (−256, 704, 1024), `LightRadius` 1200, `LightColor` (196, 226, 255), `BrightScale` 0.3, `Attenuation` `Quartic`, `AttCoefs` (1, 0, 19), `AttExps` (0, 0, −2), `LightGroup` empty, `FastLightObjects` true.
   - Object 78 is `GameStartPoint23` at (684.4248, 120, −1434.0601) with yaw −0.7330380 and `Teams` `Creed`.

4. **WORLDS3\SNOWSTORM.DAT** (a mission map with light groups, occluders and particle blockers):
   - File size 20,681,275; version 85.
   - Header offsets 3,050,093 / 3,852,065 / 3,852,177 / 3,880,582 / 3,887,254 / 3,887,602.
   - Info string empty (L = 0).
   - Extents ±(10240, 10240, 10240); world offset (0, 2048, 160).
   - World tree: N = 5461, ending at byte 815.
   - World models 212. Total polygons 36,867; total points 40,135.
   - Physics BSP (world model 211): 32,885 points, 26,139 planes, 250 surfaces, 30,859 polygons, 67,885 nodes, 203 textures.
   - World model 0: `BOSS_SlidingDoor03`, flags 0x02, 36 points, 10 planes, 3 surfaces, 40 polygons, 70 nodes, translation (84, −2964, −1046). The object of that `Name` is a `SlidingDoor` at (126.0, −2898.26, −1044.0).
   - Objects 1,065. Blind chunks 1: 100 bytes, id 0x6AAF0884.
   - Light grid counts (82, 82, 82), compressed 28,365 bytes.
   - Blockers 98; particle blockers 5.
   - Render: 133 main blocks, 211 render world models, 2,161 sections, 280 lightmaps, 194,918 render vertices, 94,662 render triangles, 6 occluders, 34 block light groups (14 named `LightGroup_H01`, 20 `LightGroup_H02`). Every intensity stream decodes to its block's vertex count, and every sub-lightmap decodes to width × height.
   - World light groups 2: `LightGroup_H01` with colour (1.0, 0.9686275, 0.9019608), offset (40, 33, 45), size (1, 1, 1); `LightGroup_H02` with the same colour, offset (39, 33, 42), size (2, 1, 3).
   - The name hash of `LightGroup_H01` is 1,196,487,219 (0x4750F233).
   - Classes: `AIVolume` 250, `SpecialFX` 145, `Light` 144, `WorldModel` 48, `SoundFX` 47, `Breakable` 45, `GameStartPoint` 16, `SlidingDoor` 15, `WeaponItem` 4, `LightGroup` 2.
   - All 16 `GameStartPoint` objects have `GameType` `CabinFever2` and `Teams` `JSF`. Object 43 is `GameStartPoint_B06` at (136.0, −3012, −1898), yaw 3.1415920, `Locked` true.
   - A sky-portal polygon has normal (1, 0, 0) and d = −10240.

5. **Lightmap format, every plaintext map:** all 13,191 shader-2 sections (summed over the 33 maps) have lightmap data, and none of the other 65,734 sections do. Every lightmap has power-of-two width and height in 16..256 and decodes to exactly width × height × 3 bytes using exactly its byte count. Shader-2 vertex UV0 values are at least 0.00195 and at most 3.022.

6. **Polygon winding, every plaintext map:** of all BSP polygons, 2,003,330 have (v1 − v0) × (v2 − v0) · n > 0, 22 have it negative, and 39 have a degenerate first triangle (|value| ≤ 1e-6).

7. **Encrypted maps (needs the archive keys):** after decryption, every `.DAT` in WORLDS, WORLDS4 and WORLDS5 has version 85 and passes test 1. WORLDS\PIAZZA.DAT is 35,895,268 bytes, the same size as a version-85 copy known from an older client.

Cases 8 onward come from the follow-up session. Map numbers in them were measured on the older regional client's 65 maps (its WAREHOUSE.DAT has the Steam copy's size, 18,076,579 bytes); behaviour cases use synthetic values.

8. **Lightmap clamping.** A shader-2 section with a 256 × 256 lightmap; a pixel whose interpolated UV0 is (3.02, 0.5) → it samples column 255 (the last), as if u were 1.0; not column 5 (which wrapping would give: 0.02 × 256 = 5.12). A vertex at u = 3.02 is not moved: a pixel halfway between it and a vertex at u = 0.98 has u = 2.0 and also reads column 255.
9. **Solid bit and rays.** WAREHOUSE's physics BSP has 4,960 polygons without the solid bit out of 37,648. Over the 65 maps: 3,387,765 physics-BSP polygons, 961,646 without the solid bit (2,831 of them invisible), 2,426,119 with it (3,360 invisible, 390 sky portals). A segment query crossing a non-solid polygon from its front side reports a hit on it; a player moving through the same polygon is not stopped. A player is stopped by a solid polygon with the invisible bit; a grenade passes it.
10. **Blockers are for players only.** A segment query crossing one of WAREHOUSE's 50 collision-blocker polygons, with no BSP polygon behind it → no hit. A player walking into it → stopped.
11. **Light-group texel.** Base lightmap texel (100, 100, 100), sub-lightmap intensity 100, group colour (1.0, 0.9686275, 0.9019608) on → (200, 196, 190). Group off → (100, 100, 100) (the base is decoded afresh). After `COLOR 128` and `ON` → colour 128/255 = 0.50196 per channel → (150, 150, 150). The same numbers for a vertex colour (100, 100, 100) with vertex intensity 100.
12. **Light-group start state.** SNOWSTORM: both `LightGroup` objects have `StartOn` false, so after the load both groups add nothing, although the render data stores them with colour (1.0, 0.9686275, 0.9019608). GRAVITYB: `Light_Grv_01` starts on, `Light_Grv_02` off.
13. **Sky camera.** WAREHOUSE: sky object `SkyPointerBox00` at (6623, 7009, −1471), `SkyDims` (256, 256, 256), `InnerPercent` 0.001 on each axis; extents ±(9952, 10080, 10112). Main camera at (0, 0, 0) → sky camera at (6623, 7009, −1471). At (9952, 10080, 10112) → (6623.256, 7009.256, −1470.744). The sky is drawn with depth off, then the depth buffer is cleared.
14. **Fog and FarZ.** WAREHOUSE's world properties: `FogEnable` true, `FogColor` (23, 23, 19), `FogNearZ` 0, `FogFarZ` 10000, `FarZ` 30000, `ClampFarZ` false. A surface colour 0.6 at view depth 5000 → f = 0.5 → 0.5 × 0.6 + 0.5 × 23/255 = 0.3451 (red, green). FarZ stays 30000; with `ClampFarZ` true it would become 20000.
15. **Lighting multiply.** Texture texel 0.8, lightmap texel 0.5, `LightmapTFactor` white → drawn 0.8 with `Saturate` 1 (default), 0.4 with `Saturate` 0. A vertex-lit texel 0.8 with vertex colour 0.25 → 0.4. With `LightmapTFactor` (128, 128, 128) on the lightmapped case: 2 × (0.8 × 0.502) × (0.5 × 0.502) = 0.2016.
16. **Dual-texture blend.** Vertex alpha 0.25, texture 0 (1, 0, 0) with alpha 1, texture 1 (0, 0, 1) → base (0.25, 0, 0.75), output alpha 0.25. Shader 8 with vertex colour (0.5, 0.5, 0.5) → (0.25, 0, 0.75). If texture 1 has the fullbright flag → base (1, 0, 0.25).
17. **Unknown shader code.** Fixture B with the section's shader code (the byte at offset 640) changed from 1 to 3 → the file parses exactly as before; the section is kept and not drawn.
18. **Blocker trailer value.** Fixture A with the collision-blocker trailer (the u32 at offset 408) set to 7 → the file parses; the value is ignored.
19. **Property length.** A real property stored with D = 8 whose first four bytes hold 0.5 → value 0.5, and the next property starts after all 8 bytes. A real with D = 2 → refused. A polygon whose vertex count byte is 41 → refused.
20. **Map identity.** A client whose WAREHOUSE.DAT differs in size from the server's joins a WAREHOUSE game → no error or warning from the file list; the client loads its own file.
21. **8-bit text.** Every byte of every class name, property name, string property, world-model name, BSP texture name and render section name in the 65 maps is below 0x80.
22. **World-model flags.** In the 65 maps: `WorldModel` with `Solid`/`Rayhit` false/false 1,324, true/true 677, false/true 184, true/false 90; `Breakable` true/true 712; solid `RotatingDoor` 257 and solid `SlidingDoor` 273. A ray stops on a false/true `WorldModel`; a player walks through it.

## Open questions

1. **Encrypted maps.** The 72 maps in V2-keyed archives could not be read here. Test case 7 checks that they are version 85 and follow this layout.
2. **Scale.** The file has no unit. Content suggests roughly centimetre-sized units (spawn points sit about 66 u above the floor; stair steps are 30 u in movement.md), but the real-world scale should be fixed by the movement or model spec, not here.
3. **`ObjectSpawnPoint`.** 60 per PvP map; its gameplay role (item or supply spawns per game type?) belongs to a gameplay spec.
4. **Occluder id.** Each occluder carries a u32 id (408 for all six in SNOWSTORM). Whether it is a name hash or an index, and what toggles occluders, is unknown.
5. **Non-solid surfaces and collision.** Whether polygons whose surface lacks the solid bit, or has the invisible bit, block players and bullets is decided in the collision code, which is outside this spec. Implementers let a polygon block players and bullets when its surface has the solid bit, ignoring the invisible bit, and asked whether collision blockers stop bullets. **Answered:** right for players (and the invisible bit indeed plays no part for them), wrong for bullets: ray queries test every BSP polygon, solid or not, so non-solid detail (about 28 % of physics-BSP polygons) stops rays; only surface types the surface table marks invisible would be skipped, and none is. Grenades skip polygons with the invisible bit. Collision blockers stop only the player mover, never bullets, grenades or cover traces. Applies in What is drawn and what is collision only, Collision blockers and cases 9–10.
6. **Surface flag bit 19.** The compiler and the engine name it differently (render-block splitter versus additive blend). It is unused in the measured maps.
7. **Lightmap and vertex-colour gamma.** The bytes are stored as above. Whether the game applies any extra scale beyond the optional 2× modulate (`LightmapTFactor` in `WorldProperties` is a candidate) was not checked. **Answered:** the 2× is on by default (console `Saturate` 1), for lightmapped and vertex-lit surfaces alike. The extra scale is `LightmapTFactor`/255 per channel, applied to both the texture and the lightmap pass (and to vertex-lit sections); every observed map stores (255, 255, 255), so it is 1. No other scale or gamma is applied to the stored bytes. Applies in Lightmaps (How it is drawn) and case 15.
8. **Other games' version-85 worlds.** Byte compatibility with other titles on the same engine version is untested.
9. **Lightmap coordinates beyond 0..1** (up to 3.02). **Answered:** clamped per pixel by the sampler (clamp addressing on the lightmap stage), neither wrapped nor clamped per vertex. Applies in Lightmaps (Mapping) and case 8.
10. **Vertex lighting.** **Answered:** one colour per vertex, the u32 at offset 28 of each 68-byte render vertex (bytes B, G, R, A). R, G, B light shaders 1 and 8 (tex × colour × 2); A is the dual-texture blend weight. Applies in Render data (Vertex), Lightmaps and case 16.
11. **Sky and fog.** How the sky portals, the sky world model and the sky object are drawn, and which world properties set fog. **Answered** in Sky: the sky is drawn first each frame, inside the screen rectangle of the visible sky portals, from a sky camera that follows the player through the sky object's inner box, with depth off and the sky fog distances, then the depth buffer is cleared and the world drawn over it; the portals themselves are never drawn. Fog is linear in depth from `FogEnable`, `FogColor`, `FogNearZ`, `FogFarZ`, with `ClampFarZ` limiting `FarZ`; the full property list is in Sky. Cases 13–14. Still open: what the sky looks like when the sky portals' rectangle covers only part of the screen and world geometry has holes (the engine's own comment says the bounds can be inaccurate); compare a screenshot looking at a sky edge.
12. **Light groups.** **Answered** in Lightmaps (Light groups): the stored colour is used until the game's `LightGroup` object sets `StartColor`/255 or black from `StartOn` during the load; map scripts switch them with `ON`, `OFF`, `TOGGLE` and `COLOR`; a change rebuilds the affected lightmaps from the stored ones plus every group's sub-lightmaps at its current colour. Cases 11–12.
13. **Second textures.** **Answered** in Second textures for dual-texture sections (vertex-alpha × texture-0-alpha blend, additive if texture 1 is fullbright), detail textures (signed add of a scaled and rotated copy of UV0) and environment maps (reflection coordinates, signed add). Open: the fullbright mask. In the reference build's draw order the fullbright second draw is overwritten by the lightmap multiply, so fullbright texels end up lit like the rest; the older order keeps them bright. Measurement: in a dark area of a map, look at a texture with the fullbright flag (`textures_dtx.md` case 5's texture) and see whether its bright parts stay at full brightness.
14. **Moving world models and volumes.** **Answered** in Moving world models and volumes: the classes, their properties, the `Solid` / `Rayhit` rules (solid world models block players, grenades and cover at every point of their motion; rays hit solid or ray-hit ones), and what each volume class does, including the endless-fall kill volume. The exact door motion (how `RotationAngles` and the times drive the angle per frame) belongs to a door spec.
15. **Map identity.** **Answered** in Map identity: no hash or size check; the server's file list carries sizes but the client ignores them and loads its own copy by name. Case 20.
16. **Malformed data and encoding.** **Answered:** the engine has only debug-build checks: a short property is read past its data, a long one uses its leading bytes, a polygon over 40 vertices overruns a fixed buffer, and the record length R is not used (the list is read in sequence); running out of file fails the load. Names and strings are plain 8-bit text, all below 0x80 in the observed maps, with no code-page conversion. Applies in Objects (Malformed records), BSP arrays, Units and conventions and cases 19 and 21.
17. **Blocker trailer and shader codes.** **Answered:** the trailer is a reserved field, read and ignored (checked for 0 only in debug builds); shader codes stop at 11 and any code other than 1, 2, 4, 8 and 9 makes a section that is kept but never drawn. Applies in Collision blockers, Render data and cases 17–18.
