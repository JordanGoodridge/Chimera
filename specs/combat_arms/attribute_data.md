# Combat Arms: attribute data

Source basis: reference material the user pointed the spec session to: the
client's and server's shared attribute-file reader (tokenizer, parser, typed
lookups), the weapon/ammo/armour manager, the server attribute manager, the
character-model manager and the client movement code that consumes their
values, plus decrypted attribute archives from two older client builds (a US
and a BR build). Every lexical rule below was checked by running probe
documents through a throwaway model of the reference tokenizer, and the
model was run over every file of the US build's attribute folder (not in
the repo). The Steam install's attribute archive was inspected only through
its plaintext directory and the plaintext tails of its payloads.
Status: draft

Sections: Summary; Units and conventions; Where the data sits; Encoding;
Document syntax; Value types; Tables, records and lookups; Defaults and
overrides; Weapons file: record kinds and load order; Field table: weapons;
Field table: ammo; Field table: armour and global spread; Field table:
movement; Reference values (one rifle); Edge cases; Quirks; Test cases;
Open questions.

---

## Summary

- All weapon, ammo, armour, character and player numbers live in plain
  text "attribute files" inside the attributes archive. The syntax is
  LithTech's INI-like attribute format: `[Section]` headers, each followed
  by `key = value` lines. Every value carries its type in its spelling
  (`7` is an integer, `7.0` a double, `"7"` a string, `<1,2,3>` a vector).
- Records are sections named prefix + index (`Weapon0`, `Weapon1`, ...,
  `Ammo0`, ...). The index is the record's runtime ID. Records link to each
  other by name (a weapon names its ammo), never by index, and never
  inherit from each other.
- Lookups are by (section, key), both case-insensitive. Typed reads are
  strict: reading an integer field whose text is `6.5` silently gives the
  field's built-in default. That strictness, the built-in defaults, and
  "first one wins" for duplicate keys are what a faithful loader must copy.
- For us: our own TOML files are the defaults; when the install's
  attribute archive can be opened, its records replace ours field by field
  (see "Defaults and overrides").

## Units and conventions

- Distances: world units (the engine's distance unit; the same unit as
  `movement.md`). Ranges, radii and speeds are world units or world units
  per second.
- Angles in weapon recoil fields: degrees (the client converts them to
  radians when it applies them).
- Times: fields whose name ends in `Delay` and are integers are
  milliseconds; fields read as numbers that hold durations (`ZoomTime`,
  `...Duration`, `...Time`) are seconds. Each field row states which.
- "Spread units" are the game's integer perturb scale; their meaning is in
  `weapon_firing.md`.
- Integers are signed 32-bit, "dword" unsigned 32-bit, "byte" unsigned
  8-bit, "double" IEEE 64-bit, "float" IEEE 32-bit.
- In this document `k` means a key, `S` a section, "a read" means the game
  asking for one (S, k) with an expected type and a fallback.

## Where the data sits

- **Archive:** the attributes archive of the install (`ATTRIBUTES.rez`
  in the game folder). Its directory is plaintext and its payloads are
  encrypted with the V1 scheme (header title `V1_RESOURCES_KEY` in the
  Steam build); see `rez_archive.md`. Older builds shipped the same
  archive unencrypted (title `LithTech Resource File`).
- **Folder:** the archive root holds exactly one directory, `ATTRIBUTES`;
  every attribute file sits directly in it. The game opens files as
  `Attributes\<NAME>.TXT` (case-insensitive). An archive whose files sit
  at the root instead of under `ATTRIBUTES` is not found by the game.
- **Files by role** (in-archive names; all in the `ATTRIBUTES` folder):

| Role | Entry | Holds |
|---|---|---|
| Weapons file | `WEAPONS.TXT` | Weapon, ammo and armour records, weapon order lists, global spread speeds, gunsmith conversion records |
| Server attributes | `SERVERBUTES.TXT` | The `[Player]` section (base speeds, see Movement) and unrelated server sections |
| Character models | `MODELBUTES.TXT` | Body type (`FigureType#`), character (`Model#`), head piece (`Hair#`) records with stamina, sprint and move-penalty values |
| Attachments | `ATTACHMENTS.TXT` | Backpack and goggle records that also carry move-penalty, sprint and stamina bonuses (attachment spec) |
| Weapon modifications | `MOD.TXT` | Per-weapon modification parts (attachment spec) |
| Shop items | `ITEMS.TXT` | Shop and inventory display data, including the stat bars `Damage`, `FireSpeed`, `FirstAccuracy`, `Recoil`, `FireRange`. Display only: gameplay never reads these. |

- Sizes for orientation: in the Steam install the weapons file is
  9,007,972 bytes and the server attributes file 2,176 bytes. In the US
  reference build the weapons file is 6,216,427 bytes and holds 1,606
  weapon, 1,474 ammo and 258 armour records; its server attributes file is
  also 2,176 bytes.
- The folder also holds files that are not in this syntax (the `.XML`
  files, and a few layout or list files read by other loaders). A loader
  for this spec only needs the four files above.

## Encoding

- **Bytes in:** the decrypted payload. The V1 decryptor used by the game
  can append up to 16 zero bytes after the real text (see `rez_archive.md`,
  "Zero-padded output"). The document ends at the first 0x00 byte or at
  the end of the buffer, whichever comes first.
- **Byte-order mark:** if the document is at least 3 bytes long and starts
  with EF BB BF, those 3 bytes are skipped. Nothing else about UTF-8 is
  special.
- **Characters:** 8-bit bytes. Keys, section names, numbers and syntax are
  ASCII. Bytes 0x80 to 0xFF are allowed only inside strings and comments;
  in the shipped files they are Korean text in the Korean Windows code page
  (CP949) and appear almost only in comments. Keep string values as raw
  bytes; decode them only for display.
- **Line ends:** shipped files use CR LF. CR is plain whitespace. Only LF
  ends a `//` comment.
- **Whitespace:** space (0x20), tab (0x09), CR (0x0D), LF (0x0A). Any
  other control byte outside a string or comment is an error.

## Document syntax

A document is a sequence of tokens separated by optional whitespace and
comments. Tokens may also touch with no space between them (`[A]x=1[B]y=2`
is valid).

- **Comments:** `//` to the next LF (or the end); `/*` to the next `*/`,
  across lines, not nested (the first `*/` ends it). A lone `/` is an
  error. An unterminated `/*` is an error. Comments may follow a section
  header or a value on the same line.
- **Section header:** `[`, an identifier, `]`. Nothing may come between
  `[` and the identifier: a space, digit, `+` or `-` right after `[`
  starts a range literal instead, so `[ A ]`, `[1]` and `[9a]` are errors.
  Newlines between the brackets and the name are tolerated.
- **Identifier** (section names and keys): one or more of `A-Z a-z 0-9 _`.
  It may start with a digit as long as it is not a pure number (`9a`,
  `1x`, `_1` are identifiers; `12` is an integer). `.`, `-`, `$`, `'` and
  all other punctuation end it, so `a.b = 1` and `a-b = 1` are errors.
- **Statement:** identifier, `=`, one value. One statement per key; line
  breaks are not significant (`[A] x = 1 y = 2` is two statements).
- **Document:** zero or more sections. The first token must be a section
  header or the end. A statement before the first header makes the whole
  document invalid. A section may be empty. An empty document, or one with
  only comments, is valid and has no sections.
- **Errors:** any lexical or grammar error makes the whole document
  invalid. The game keeps nothing from a document that fails, and the
  manager that asked for it fails to start (for the weapons file this is
  fatal for the client).

## Value types

The spelling of a value decides its stored type. Lexical forms:

| Written as | Stored type | Rule |
|---|---|---|
| `7`, `-5`, `+5`, `00012` | integer | Optional sign, then decimal digits, nothing else. Converted with C `atoi` semantics. |
| `1.5`, `-0.25`, `1.5e3`, `1.5E-3` | double | Optional sign, digits, `.`, at least one digit, optional exponent (`e`/`E`, optional sign, digits). |
| `1.5f`, `-1.5e+2F` | float | A double spelling followed directly by `f` or `F`. Converted to 32-bit. |
| `(DWORD)7` | dword | The cast word in any letter case, optional spaces inside the parentheses (`( DWORD )`), then an unsigned integer (no sign). Decimal. |
| `(BYTE)300` | byte | Same, then an unsigned integer; the value is taken modulo 256 (300 becomes 44). |
| `(FLOAT)2.25` | float | Same, then a double spelling (a decimal point is required; `(FLOAT)2` is an error; `(FLOAT)2.5f` is accepted). |
| `TRUE`, `false`, `True` | bool | The two words in any letter case. Words like `TRUEX` are identifiers, which are not values (error). |
| `"text"` | string | Any bytes except CR, LF and 0x00 between double quotes. `""` inside stands for one `"`. No other escapes; a backslash is an ordinary byte. |
| `<x, y, z>` | vector | Exactly three numbers, comma separated. Spaces may surround numbers and commas; tabs and newlines may not. Each number: optional sign, digits, optional `.digits`, optional exponent (so `1e3` is fine here). Stored as three doubles. `<1.,2,3>`, `<.5,1,1>`, `<1.5f,2,3>`, two or four components are errors. |
| `(a, b)` | point | Two integers (optional sign), spaces allowed, no decimals. |
| `(a, b, c, d)` | rect | Four integers, as for point. Three or five components are errors. |
| `[a, b]` | range | Two numbers as for vector components. |

Not values (each is an error): `1e5` and `12ab` (identifiers), `.5`, `5.`,
`-.5`, `- 5`, single quotes, `0x10`, `1/2`, an identifier on the right of
`=`.

Number range: integers beyond 32 bits do not occur in shipped data; clamp
them to the 32-bit range. Token length: the reference holds a token in a
4,096-byte buffer; no shipped token comes close. Our reader must not limit
it, but see the 127-byte string limit under "Tables, records and lookups".

## Tables, records and lookups

**Storage.** A parsed document is a map from section name to a map from
key to typed value. Section names and keys compare case-insensitively
(ASCII letters only; bytes above 0x7F compare exactly). Key order is not
kept and nothing depends on it.

**Duplicates.**
- Two sections whose names differ only in case, or not at all: the
  document is invalid.
- Two keys in one section that match case-insensitively: the first one in
  the file is kept, later ones are ignored, and parsing continues. Shipped
  files rely on this (for example a repeated key in a quest record).

**Typed reads.** The game always asks for one type and gives a fallback.
The stored type must match, with one exception for numbers:

| Read as | Accepts stored | Otherwise |
|---|---|---|
| integer | integer only | fallback |
| number (float or double) | integer, float, double | fallback |
| dword | dword only | fallback |
| byte | byte only | fallback |
| bool | bool only | fallback |
| string | string only | fallback |
| vector, point, rect, range | that exact type | fallback |

So `MinPerturb = 3.0` read as integer gives the default, `TRUE` read as
integer gives the default, `1` read as bool gives the default, and
`Range = "3000"` read as integer gives the default. A missing key also
gives the fallback. Reads never fail loudly; at most a debug message.

**Fixed-size string reads.** Most string fields are copied into a 128-byte
buffer. A stored string of 128 bytes or more does not fit, and the read
gives the field's fallback (normally the empty string) instead of a
truncated value. Treat "127 bytes" as the maximum usable length of a name
or path field.

**`$` strings (client only).** A string value starting with `$` (for
example an item's display name) is a key into the localised string table,
resolved when the client reads it. The server and our loader keep it as is.

**Record enumeration.** A record kind is read by trying `Prefix0`,
`Prefix1`, ... and stopping at the first index whose section does not
exist. Indices are decimal with no leading zeros. A gap ends the list:
with `Ammo0`, `Ammo1` and `Ammo3`, only two ammo records exist and `Ammo3`
is ignored. A section that exists but is empty still counts as a record
(with every field at its default). The position in the sequence is the
runtime ID.

**Lists inside a record.** A list field is read the same way: keys
`Name0`, `Name1`, ... in one section, stopping at the first missing index.
The game has three kinds of list read:
- string list: each element read as string; an element stored as a number
  reads as the empty string (it is then converted by the consumer, so it
  becomes 0);
- integer list: each element read as integer, fallback 0;
- number list: each element read as number, fallback 0.0.

A list with no element 0 is empty, and an empty list leaves the field at
its previous value (its default).

**Links between records.** Links are by name: the string field of one
record equals the `Name` field of another, compared case-insensitively.
The first record in index order whose `Name` matches wins; a record with
an empty name never matches. Ammo names are not unique in shipped data
(two ammo records share one name in the US build), so "first wins" is
observable.

**After loading** the weapons, models and server managers build their
records once at start-up, and gameplay reads the built records, not the
text.

## Defaults and overrides

Order, lowest priority first:

1. **Built-in default** for each field (the "Default" column in the field
   tables). Every record starts from these.
2. **File value**, when the key exists in the record's section and has an
   accepted type (see "Typed reads").
3. **Event text-change overwrite**, a runtime mechanism: an event
   configuration supplies (file, section, key, value-text) entries, which
   are applied right after that file is parsed. An entry only changes a
   key that already exists, and converts the text according to the key's
   stored type. No shipped attribute file contains such entries; they come
   from server-side configuration. Implementing this layer is optional; if
   implemented, copy the quirks listed under "Quirks".

There is no inheritance between records: no record names a parent, and no
record copies another's fields. A weapon's `Name` and an ammo's `Name` are
read once and never changed later.

**Our loader (project rule).** We ship our own records
(`games/combat_arms/data/weapons/*.toml`). When the install's attributes
archive can be read:
- match install records to ours by `Name`, case-insensitively;
- for a matched record, every field present in the install with an
  accepted type replaces ours; a field missing from the install, or
  present with a rejected type, takes the **built-in default** from this
  spec, not our TOML value (that is what the game shows);
- install records with no match in our data are new records;
- if the install document fails to parse, use our data unchanged and log
  the error with the document name and line;
- runtime IDs come from the install's record order and must not be
  stored in our data.

## Weapons file: record kinds and load order

Sections in the weapons file, in the order the game reads them:

1. `DSight0..` crosshair styles (presentation only).
2. `Gear0..` armour records.
3. `Ammo0..` ammo records. Zero ammo records make loading fail.
4. `Weapon0..` weapon records. Zero weapon records make loading fail.
   Ammo must already be loaded, because a weapon resolves its
   `AmmoName#` list by name.
5. `[WeaponOrder]`: `Class0..` is a string list of class names (shipped:
   `Primary`, `Secondary`, `Melee`, `Throw`, `Special`, `Mission`). For each
   class name C, the string list `C0..` names the weapons in that class, in
   inventory order. A name that matches no weapon makes loading fail.
   `AutoSwitchWeapon0..` names weapons in auto-switch priority order; an
   unknown name there is ignored.
6. `[Perturb]` global spread speeds (see "Field table: armour and global
   spread"). Each key is read only if present.
7. `[BombExceptList]` integer list `WeaponID0..` (weapon IDs exempt from a
   bomb-mode rule; not needed for the slice).
8. `Convert0..` gunsmith conversion records and `PatternWeapon0..`
   records (not needed for the slice).
9. `[DeditWeapon]` string list `Weapon0..` (weapons offered to the level
   editor; ignore).

Both the client and the server read the weapons file. The server uses the
damage, range and ammo amounts; the client uses spread, recoil, sounds,
models and HUD fields; both use clip sizes and fire timing.

## Field table: weapons

Section `Weapon<N>`. Only fields that affect behaviour or that the slice
needs are listed; model, skin, sound, effect and HUD-image paths are
strings read with an empty default and are listed at the end in one row.

Type column: "int" = integer read (strict), "num" = number read (accepts
integer, float, double), "bool" = bool read (strict), "str" = string read,
"vec" = vector, "str list" / "int list" / "num list" = list reads.

| Key | Type | Unit | Default | Meaning | Read by |
|---|---|---|---|---|---|
| `Name` | str (≤127 bytes) | | empty | Unique weapon name; target of all links (order lists, inventory, items). Never changed after load. | everything that looks a weapon up |
| `Guntype` | int | enum | 0 | Weapon class: 0 knife, 1 pistol, 2 shotgun, 3 SMG, 4 assault rifle, 5 machine gun, 6 sniper rifle, 7 thrown, 8 mission bomb, 9 rocket, 10 mine, 11 heartbeat sensor, 12 air strike, 13 double barrel, 14 snowball gun, 15 special grenade launcher, 16 medic kit, 17 install controller, 18 install bomb, 19 first-aid kit, 20 turret, 21 crossbow (later values exist; treat unknown as "other") | class-specific firing, inventory slot rules, bots |
| `ClientWeaponType` | int | enum | 1 | Client weapon behaviour kind | client weapon |
| `AniType` | int | enum | 5 | Animation set for view and third-person models | animation |
| `LowerAniType` | int | enum | 0 | Lower-body animation set | animation |
| `ShotsPerClip` | int | rounds | 100 | Magazine size | firing, reload, HUD |
| `AmmoName0..9` | str list (max 10) | | none | Ammo this weapon can use, by ammo `Name`. Names that match no ammo are dropped and the list closes up. The first remaining entry is the default ammo. | firing, damage |
| `InfiniteAmmo` | int (0/1) | | 0 | Never runs out | ammo |
| `ReloadPerBullet` | int (0/1) | | none (see Edge cases) | Reloads one round at a time (shotguns) | reload |
| `VectorsPerRound` | int | rays | 1 | Hitscan rays per shot (pellets) | firing |
| `Range` | int | world units | 3000 | Maximum hitscan distance | firing |
| `Effectrange0..2` | str list | world units | none | Upper bounds of the three damage-falloff bands, read from strings and truncated to integers. Paired with the ammo's `Rangedamage0..2`. | damage (server) |
| `Alt_Effectrange0..2` | str list | world units | none | Same, for the secondary attack (knives) | damage (server) |
| `DamageRFacter` | num | fraction | 0.1 | Random damage spread: each hit is scaled by a uniform random factor in [1 − f, 1 + f]. Values outside [0, 1] are replaced by 0.2 when used. | damage (server) |
| `FireDelay` | int | ms | 30 | Minimum time between shots. Negative values become 0. | firing |
| `FireAniRate` | num | multiplier | 0.0 | Fire animation rate override (0 = none) | firing, animation |
| `RapidFireCount` | int | shots | 1 | Shots per trigger pull: 0 = fully automatic (fires while held), 1 = semi-automatic, n > 1 = stops after n shots | firing, HUD fire-mode icon |
| `ContinuousFire` | int | | 0 | Only the value 1 enables continuous-fire mode | firing |
| `HudFireMode` | int | enum | 0 | Fire-mode icon on the HUD | HUD |
| `TripleShotMode` | int (0/1) | | 0 | Weapon has a burst mode | firing |
| `TripleShotRoundNum` | int | rounds | 3 | Rounds per burst | firing |
| `TripleShotDelay` | int | ms | 300 | Delay between bursts | firing |
| `TripleShotSelectDelay` | int | ms | 400 | Delay when switching into burst mode | firing |
| `TripleShotMinPerturb`, `TripleShotMaxPerturb` | int | spread units | 7, 100 | Spread limits in burst mode | spread |
| `TripleShotPerturbIncSpeed`, `TripleShotPerturbDecSpeed` | num | per shot / per second | 1.0, 1.0 | Spread growth and recovery in burst mode | spread |
| `TripleShotMaxCamRecoilPitch`, `TripleShotBaseCamRecoilPitch` | num | degrees | 50.0, 1.0 | Recoil in burst mode | recoil |
| `MinPerturb` | int | spread units | 100 | Spread floor, standing | spread |
| `MaxPerturb` | int | spread units | 100 | Spread ceiling | spread |
| `DuckPerturb` | int | spread units | 100 | Spread floor, crouched | spread |
| `MoveDuckPerturb` | int | spread units | 100 | Spread floor, crouched and moving | spread |
| `ZoomedMinPerturb`, `ZoomedMaxPerturb` | int | spread units | 1, 500 | Spread limits while zoomed | spread |
| `PerturbIncSpeed`, `PerturbDecSpeed` | num | see firing spec | 1.0, 1.0 | Spread growth per shot and recovery rate | spread |
| `ZoomedPerturbIncSpeed`, `ZoomedPerturbDecSpeed` | num | see firing spec | 3.0, 3.0 | Same, zoomed | spread |
| `DynamicPerturbFactor` | num | multiplier | 1.0 | Scales movement-driven spread | spread |
| `DynnamicPerturbFactorRun`, `...Walk`, `...Jump` | num | multiplier | 1.0 each | Per-state factors. The key really has a doubled `n`; no shipped record uses it. | spread |
| `ZoomTime` | num | s | 0.5 | Time to zoom in | zoom |
| `MovePenalty` | num | fraction | 0.01 | Fraction of top speed lost while this weapon is held | movement |
| `ZoomedMovePenalty` | num | fraction | 0.7 | Same, while zoomed | movement |
| `MaxCamRecoilPitch` | num | degrees | 10.0 | Accumulated upward view-kick cap | recoil |
| `BaseCamRecoilPitch` | num | degrees | 1.2 | Upward view kick per shot | recoil |
| `BaseCamRecoilAccelPitch` | num | degrees | 0.0 | Extra kick added per consecutive shot | recoil |
| `MaxCamRecoilYaw`, `BaseCamRecoilYaw` | num | degrees | 0.0, 0.0 | Sideways kick cap and per-shot kick | recoil |
| `MaxCamRecoilWag` | num | see firing spec | 30.0 | Side-to-side wobble cap | recoil |
| `BaseCamRecoilWagL`, `BaseCamRecoilWagR` | num | see firing spec | −1.0, 1.0 | Wobble step left and right. Shipped data usually stores L positive and R negative. | recoil |
| `MaxRandomPitch`, `MaxRandomWag` | num | see firing spec | 0.032, 0.015 | Random per-shot kick | recoil |
| `BaseCamBackJitter`, `BackJitterRecover` | num | see firing spec | 0.5, 3.0 | Backward camera jitter and its recovery | recoil |
| `CameraSwayXSpeed`, `CameraSwayYSpeed`, `CameraSwayXFreq`, `CameraSwayYFreq` | num | | 12.0, 2.0, 6.0, 20.0 | Scope sway | zoom |
| `DisableCrosshair` | int | | 0 | Non-zero hides the crosshair | HUD |
| `CrossHairType` | int | enum | 0 | Crosshair style | HUD |
| `NoSelect` | int | | 0 | Cannot be selected by the player | inventory |
| `GrenadeSpeed` | num | multiplier | 1.0 | Throw speed scale | grenades |
| `GrenadeAttach` | int | | 0 | Thrown object sticks to what it hits | grenades |
| `WideFireMaxNum`, `WideFireAngleDegree` | int | count, degrees | 0, 0 | Fan-shaped multi-shot | firing |
| `FireSndRadius` | int | world units | 1000 | Fire sound radius | audio, bots |
| `ReloadSndRadius`, `SelectSndRadius` | int | world units | 300, 300 | Reload and select sound radii | audio |
| `SilencedFireRadius1..10` | int | world units | 0 each | Fire sound radius with silencer n (each key read on its own, not as a list) | audio, bots |
| `DropByPlayerDead` | bool | | true | Dropped on death | pickup/drop |
| `DamageWeapon` | bool | | true | Deals damage | damage |
| `CheckReloadAniTime` | bool | | true | Reload length follows the animation | reload |
| `Hidden` | bool | | false | Hidden from lists | UI |
| `Pos`, `MuzzlePos`, `BreachOffset`, `HHModelScale`, `Recoil`, `MuzzlePVPosOff`, `MuzzlePVRotOff`, `MuzzleHHPosOff`, `MuzzleHHRotOff`, `ScopePVPosOff`, `ScopePVRotOff`, `ScopeHHPosOff`, `ScopeHHRotOff` | vec | world units / degrees | all zero | View-model and world-model placement | presentation |
| `PVModel*`, `PVSkin0..`, `PVRenderStyle0..`, `HHModel`, `HHSkin0..`, `HHRenderStyle0..`, `FireSnd`, `Alt_FireSnd`, `DryFireSnd`, `SelectSnd`, `DeselectSnd`, `ReloadSnd1..3`, `SilencedFireSnd1..10`, `PVMuzzleFXName`, `HHMuzzleFXName`, `Hudactiveimage`, `Hudnoneactiveimage`, `Hudweaponnameimage`, `Weaponkillimage`, `DotSightName` | str | | empty | Asset references by in-archive path or effect name | presentation |

## Field table: ammo

Section `Ammo<N>`. Damage is per ray (per pellet).

| Key | Type | Unit | Default | Meaning | Read by |
|---|---|---|---|---|---|
| `Name` | str (≤127 bytes) | | empty | Name that weapons link to. Never changed after load. | weapon load |
| `Type` | int | enum | unknown | 0 hitscan, 1 projectile, 2 grenade, 3 gadget, 4 mission bomb | firing |
| `Priority` | num | | −1.0 | Ammo preference order | inventory |
| `MaxAmount` | int | rounds | 0 | Reserve cap | ammo |
| `SpawnedAmount` | int | rounds | 0 | Reserve on spawn | ammo |
| `SelectionAmount` | int | rounds | 0 | Reserve given with the weapon | ammo |
| `InstDamage` | int | hit points | 0 | Damage per hit before range, zone and random scaling | damage (server) |
| `Alt_InstDamage` | int | hit points | the record's `InstDamage` | Secondary-attack damage. The fallback is this record's `InstDamage` value. | damage (server) |
| `InstDamageType` | int | enum | −1 (invalid) | Damage type: 0 unspecified, 1 bleeding, 2 bullet, 3 burn, 5 crush, 7 explode, 9 poison, 12 stun, 13 melee (others exist) | damage (server) |
| `Rangedamage0..2` | str list | multiplier | none | Damage multiplier in each falloff band, read from strings as decimals | damage (server) |
| `Alt_Rangedamage0..2` | str list | multiplier | none | Same for the secondary attack | damage (server) |
| `AreaDamage`, `AreaDamageRadius` | int | hit points, world units | 0, 0 | Splash damage and radius | explosions |
| `AreaDamageType` | int | enum | −1 | Splash damage type | explosions |
| `ProgDamage`, `ProgDamageDuration`, `ProgDamageRadius`, `ProgDamageLifetime` | num | hit points per second?, s, world units, s | 0, 0, 0, 0 | Damage over time | damage (server) |
| `ProgDamageType` | int | enum | −1 | Damage-over-time type | damage (server) |
| `StunnedTime`, `StunnedMovePenalty` | num | s, fraction | 0, 0 | Stun on hit | movement |
| `CanAdjustInstDamage` | int | | 1 | Whether damage modifiers apply | damage |
| `FirePerturb` | num | spread units per shot | 0.0 | Spread added per shot, standing | spread |
| `FireMovePerturb`, `FireDuckPerturb`, `FireMoveDuckPerturb` | num | spread units per shot | 0.0 each | Same while moving, crouched, crouched and moving | spread |
| `WalkPerturb`, `RunPerturb`, `JumpPerturb` | num | see firing spec | 0.0 each | Movement spread contribution | spread |
| `PerturbRecoil` | num | multiplier | 0.0 | Spread-to-recoil coupling | spread |
| `Zoomed...` (each of the eight keys above with a `Zoomed` prefix) | num | | 0.0 each | Same, zoomed | spread |
| `DecDelay` | int | ms | 150 | Wait after a shot before spread starts to recover | spread |
| `JitterDecDelay` | int | ms | 600 | Same for camera jitter | recoil |
| `JitterWaitRecover`, `JitterRecover` | num | see firing spec | 0.1, 0.3 | Jitter recovery | recoil |
| `DamageMovePenalty` | num | unconfirmed (see Open questions) | 0.0 | Slow-down applied to whoever is hit | movement (victim) |
| `DamageMovePenaltyDuration` | num | s | −1.0 | How long that slow-down lasts | movement (victim) |
| `Health` | num | hit points | 0.0 | Hit points of a placed object fired with this ammo | gadgets |
| `ImpactFXName`, `FireFXName`, `TracerFXName`, `ProjectileFXName`, ... | str | | see Quirks | Effect names (effects spec) | presentation |

The four default spread-recovery values (`DecDelay` 150, `JitterDecDelay`
600, `JitterWaitRecover` 0.1, `JitterRecover` 0.3) apply only when the key
is missing. When the key is present but has the wrong type, the value is
whatever the record held before, which for a fresh record is undefined
(see Edge cases).

**Damage by range** (for orientation; the full rule belongs to
`weapon_firing.md`). With band limits $E_0, E_1, E_2$ from the weapon and
multipliers $R_0, R_1, R_2$ from the ammo, and $d$ the distance from the
fire position to the impact:

```math
m(d) = \begin{cases}
R_0 & 0 \le d \le E_0 \\
R_1 & E_0 < d \le E_1 \\
R_2 & E_1 < d \le E_2 \\
1 & \text{otherwise}
\end{cases}
```

$E_i$ is the string converted to an integer (decimal part dropped), $R_i$
the string converted to a double. Banding applies only when both lists have
at least three elements; elements past the third are ignored. Otherwise the
damage is not range-scaled at all.

## Field table: armour and global spread

**`Gear<N>` (armour).** `Name` (str), `Armor` (num, armour points,
default 0), `Health` (num, default 0), `Exclusive` (int, default 1),
`ProtectionType0..` (int list: damage types this gear protects against),
`NodeHeadProtection0..`, `NodeTorsoProtection0..`, `NodeArmProtection0..`,
`NodelegProtection0..` (num lists, one element per protection type,
fraction of damage absorbed, each clamped to [0, 1]), `Consumable0..` (int
list). Read by damage (server); details belong to the damage spec.

**`[Perturb]`** (each key read as number only if present):

| Key | Default | Meaning |
|---|---|---|
| `MovementPerturbIncreaseSpeed` | 3.0 | How fast movement spread builds |
| `MovementPerturbDecreaseSpeed` | 0.7 | How fast movement spread fades |
| `FiringPerturbIncreaseSpeed` | 4.0 | How fast firing spread builds |
| `FiringPerturbDecreaseSpeed` | 1.0 | How fast firing spread fades |
| `PerturbRotationEffect` | 2.0 | Spread from turning the view |

Shipped values: 3.0, 3.0, 3.0, 9.0, 3.0.

## Field table: movement

Movement numbers come from three places. Only some of them are attribute
data.

**1. Not attribute data.** The client's base speeds are engine console
variables with built-in values: walk 70, forward/back/side run 285 each,
jump 330, crouched 50 (world units per second). They are listed here so
that nobody looks for them in the archive; `movement.md` owns them.

**2. `[Player]` in the server attributes file.**

| Key | Type | Unit | Shipped | Missing | Read by |
|---|---|---|---|---|---|
| `WalkSpeed` | num | world units/s | 90 | 0 | server player object |
| `RunSpeed` | num | world units/s | 240 | 0 | server player object |
| `JumpSpeed` | num | world units/s | 485 | 0 | server player object |
| `LadderSpeed` | num | world units/s | 100 | 0 | server player object |
| `WalkFootstepVolume`, `RunFootstepVolume`, `CrouchFootstepVolume` | num | 0..1 | 0.0, 1.0, 0.0 | 0 | footstep audio |
| `DefaultWeapon` | str | | `M9` | empty | not traced |
| `SuperJumpSpeed`, `DashSpeed`, `Maxstamina`, `SwimSpeed`, `ZipCordSpeed` | num | | 485, 1.5, 100, 150, 400 | | nothing in the player code reads these |

"Missing" here: the read has no meaningful fallback (it returns the
smallest positive double, which becomes 0 as a float). Whether the four
server speeds ever reach the client's movement is an open question.

**3. Character records in the models file.**

`FigureType<N>` (body type; shipped: 0 "Male", 1 "Female", identical
numbers):

| Key | Type | Unit | Default if missing | Shipped | Meaning | Read by |
|---|---|---|---|---|---|---|
| `Name` | str | | empty | Male / Female | | |
| `Sex` | str | | `Female` | Male / Female | `Female` in any letter case gives female, anything else male (so a missing key gives female) | sounds |
| `DashSpeed` | num | multiplier | 1.4 | 1.0 | Sprint top speed = run speed × this × (1 + sum of gear sprint bonuses) | movement (sprint) |
| `Maxstamina` | int | stamina points | none (very negative) | 100 | Base stamina before gear bonuses | movement (stamina) |
| `DashGauge` | int | | none | 30 | Stamina cost parameter of sprinting | movement (stamina) |
| `DashJumpGauge` | | | | 30 | Present in data, but this client never reads it | nothing |
| `GaugeRec` | int | | none | 25 | Stamina recovery parameter | movement (stamina) |
| `VariableJump` | num | fraction | 0 | 0.0 | Jump speed × (1 + this + the model's own value) | movement (jump) |
| `VariableSpeed`, `VariableDamage`, `VariableWeight` | num | fraction | 0 | 0.0 | Per-body adjustments | movement, damage |
| `WalkPerturb`, `RunPerturb`, `JumpPerturb`, `FirePerturb`, `FireDuckPerturb`, `FireMoveDuckPerturb`, `FireMovePerturb`, `Recoil` | num | | 0 | 0.0 | Per-body spread and recoil adjustments | spread |
| `AttackedPush`, `StiffDuration` | num | , s | 0 | 0.0 | Knock-back and hit stun | movement |
| `HP`, `Speed`, `Jump`, `Shooting`, `Damagedown`, `WeaponFenalty` | num, truncated to int | | 0 | 100, 90, 57, 70, 0, 10 | Body stats. Their consumers were not traced; `Speed` is not the movement speed (see part 1). | not traced |
| `HPIncPercent` | num | | 0 | 1 | | not traced |

"none (very negative)" means the read falls back to the smallest 32-bit
integer; shipped data always has the key.

`Model<N>` (character) and `Hair<N>` (head piece), movement-related keys
only, each read as number with fallback 0.0 unless noted:

| Key | In | Meaning | Read by |
|---|---|---|---|
| `FigureType` | Model (int, fallback 0) | Index of the body-type record | everything above |
| `MovePenalty` | Model, Hair | Fraction of top speed lost (negative = faster). Shipped range about −0.19 to 0.2. | movement |
| `DashSpeed` | Model, Hair | Added to the sprint bonus sum | movement (sprint) |
| `MaxStamina` | Model, Hair | Added to the stamina multiplier: max stamina = `Maxstamina` × (1 + sum of these bonuses) | movement (stamina) |

The movement code adds the held weapon's `MovePenalty` to the model,
head-piece, attachment and modification penalties, clamps the sum to
[−1, 1], and multiplies the top speed by one minus it. That arithmetic
belongs to `movement.md`; this spec only fixes where the inputs come from.

## Reference values (one rifle)

From the US reference build (the Steam build may differ; the heavy test
reads the real install). Weapon record `M4A1` with ammo `5.56mm M4A1`:

| Key | Value | Key | Value |
|---|---|---|---|
| `Guntype` | 4 | `ShotsPerClip` | 30 |
| `RapidFireCount` | 0 | `FireDelay` | 0 |
| `Range` | 3500 | `VectorsPerRound` | 1 |
| `MinPerturb` / `MaxPerturb` | 3 / 100 | `DuckPerturb` / `MoveDuckPerturb` | 2 / 3 |
| `PerturbIncSpeed` / `PerturbDecSpeed` | 1.3 / 0.3 | `ZoomedMinPerturb` / `ZoomedMaxPerturb` | 2 / 30 |
| `MovePenalty` / `ZoomedMovePenalty` | 0.1 / 0.1 | `ZoomTime` | 0.15 |
| `BaseCamRecoilPitch` / `MaxCamRecoilPitch` | 1.3 / 5 | `BaseCamRecoilAccelPitch` | 0.2 |
| `MaxCamRecoilWag` | 15 | `BaseCamRecoilWagL` / `R` | 1.2 / −1.2 |
| `MaxRandomPitch` / `MaxRandomWag` | 0.02 / 0.01 | `BaseCamBackJitter` / `BackJitterRecover` | 0.0 / 1.0 |
| `DamageRFacter` | 0.05 | `Effectrange0..2` | "2500", "3000", "3500" |
| ammo `InstDamage` | 35 | ammo `InstDamageType` | 2 |
| ammo `MaxAmount` / `SpawnedAmount` / `SelectionAmount` | 120 / 120 / 120 | ammo `Rangedamage0..2` | "1.0", "0.7", "0.35" |
| ammo `FirePerturb` / `FireMovePerturb` | 0.30 / 0.38 | ammo `FireDuckPerturb` / `FireMoveDuckPerturb` | 0.30 / 0.38 |
| ammo `WalkPerturb` / `RunPerturb` / `JumpPerturb` | 0.04 / 0.25 / 0.70 | ammo `PerturbRecoil` | 1.0 |
| ammo `DecDelay` / `JitterDecDelay` | 95 / 95 | ammo `JitterWaitRecover` / `JitterRecover` | 0.1 / 0.2 |
| ammo `DamageMovePenalty` / `DamageMovePenaltyDuration` | 40 / 0.3 | ammo `Alt_InstDamage` | missing, so 35 |

## Edge cases

- **Wrong type on an integer field.** Shipped data has a few integer
  fields written with decimals (in the US build: a pistol with
  `DuckPerturb=6.5` and `MoveDuckPerturb=4.8`, an SMG with
  `ZoomedMinPerturb=6.9` and `ZoomedMaxPerturb=19.5`, a turret with
  `FireDelay=0.9`). The game uses the built-in default for those fields
  (100, 100, 1, 500, 30). Copy this.
- **Bool fields written as numbers.** `DropByPlayerDead = 0` is ignored
  (the field stays true). Shipped data writes these as `TRUE`/`FALSE` in
  any case.
- **Unquoted list elements.** `Effectrange0 = 2500` (no quotes) reads as
  the empty string, which becomes 0. Shipped data always quotes the band
  lists.
- **Gaps** end any enumeration (records and lists).
- **Weapon with no resolvable ammo.** If `AmmoName0..` exists but no name
  resolves, the game's default-ammo slot is undefined. Treat it as a
  weapon with no ammo and log it; shipped data has none.
- **Weapon with no `Effectrange` list.** One shipped record has none; its
  damage is never range-scaled.
- **`ReloadPerBullet` missing.** The built-in value is not set, so it is
  undefined in the game. Every shipped weapon record has the key. Use 0.
- **Ammo spread-recovery keys with a wrong type** (for example
  `DecDelay = 95.0`): the game keeps an undefined value. Shipped data has
  none; use the missing-key default.
- **An empty record** (`[Weapon7]` with no keys) is a valid record with
  all defaults and an empty name; nothing can link to it.
- **A 0x00 byte inside the text.** The document ends there. (The game's
  reader actually skips ahead to its next 255-byte read boundary and goes
  on, which is not worth copying; shipped files have no 0x00 except the
  decryptor's trailing padding.)
- **Files that fail.** In the US build, eight files in the folder do not
  parse with this grammar (the three `.XML` files, four layout or list
  files and the skills file). None of them is in the "Files by role"
  table.

## Quirks

- **Strict integer reads** (above). Keep them: they change real weapons'
  spread.
- **First duplicate key wins.** Keep.
- **Name links are case-insensitive and first-match.** Keep.
- **Effect names with empty values.** For several ammo effect keys, a key
  that is present but empty falls back to a fixed effect (impact
  `BulletFmj`, underwater impact `UWBullet`, projectile `G62`, fire
  `5.56`, tracer `Standard`); a missing key leaves no effect. This matters
  only for effects.
- **Event overwrite bugs** (only if that layer is implemented): an
  overwrite of a double or float key converts the *key name* instead of
  the value text (so it normally writes 0); an overwrite of a dword key
  converts the *section name*; a byte key is rewritten as an integer;
  rect, point, vector and range values are re-read from the value text
  with the bracket forms above; a bool becomes true only for the text
  `TRUE` in any case.
- **Base speeds in `[Player]`** (90/240/485) differ from the client's
  built-in movement speeds (70/285/330). See Open questions.

## Test cases

Each fixture is a byte string; "␍␊" means the two bytes 0D 0A. A reader
must produce exactly the listed values. "Rejected" means the whole document
is invalid. Read results use the field tables' fallbacks.

1. **Minimal.** `[A]␍␊k=1␍␊` → one section `A`; `A.k` is integer 1.
2. **Every type.** One section `[T]` with these statements, one per line:

   | Statement | Stored type | Value |
   |---|---|---|
   | `i = -5` | integer | −5 |
   | `p = +5` | integer | 5 |
   | `z = 00012` | integer | 12 |
   | `d = 1.5` | double | 1.5 |
   | `e = 1.5E-3` | double | 0.0015 |
   | `f = -1.5e+2f` | float | −150.0 |
   | `w = (DWORD)4000000000` | dword | 4,000,000,000 |
   | `b = (BYTE)300` | byte | 44 |
   | `c = ( dword )7` | dword | 7 |
   | `g = (FLOAT)2.25` | float | 2.25 |
   | `t = tRuE` | bool | true |
   | `s = "a ""b"" c\d"` | string | `a "b" c\d` (bytes 61 20 22 62 22 20 63 5C 64) |
   | `n = ""` | string | empty |
   | `v = < 1.5 ,-2, 3e2 >` | vector | (1.5, −2.0, 300.0) |
   | `q = (-1, 2)` | point | (−1, 2) |
   | `r = (1,2,3,4)` | rect | (1, 2, 3, 4) |
   | `h = [ -1.5 , +2 ]` | range | (−1.5, 2.0) |

3. **Comments and layout.** `// head␍␊[A] // tail␍␊/* one␍␊two */ k = 7 /*x*/ j=8[B]m="u"//e` →
   `A.k` = 7, `A.j` = 8, `B.m` = "u".
4. **Line comment ends only at LF.** `[A]␍␊//c␍k=1␍␊` → section `A` with no
   keys (the CR does not end the comment, so `k=1` is inside it).
5. **Empty documents.** Zero bytes → valid, no sections. `// x␍␊/* y */` →
   valid, no sections.
6. **BOM and padding.** EF BB BF `[A]␍␊k="v"␍␊` followed by 00 00 00 00 →
   same result as without the BOM and zeros: `A.k` = "v".
7. **Case-insensitive lookup.** `[Weapon0]␍␊Name="X"␍␊` → read (`WEAPON0`,
   `name`) as string gives "X"; read (`weapon0`, `NAME`) gives "X".
8. **Duplicate key.** `[A]␍␊k=1␍␊K=2␍␊` → valid; `A.k` = 1.
9. **Duplicate section.** `[A]␍␊k=1␍␊[a]␍␊j=2␍␊` → rejected.
10. **Rejected documents.** Each line below, as the body after `[A]␍␊`
    unless it says otherwise, makes the document rejected:
    `k=1e5`; `k=.5`; `k=5.`; `k=- 5`; `k=(FLOAT)2`; `k=(INT)5`;
    `k='x'`; `k="a` CR `b"`; `k=<1,2>`; `k=<1,2,3,4>`; `k=<1.,2,3>`;
    `k=(1,2,3)`; `k=(1.5,2)`; `k=TRUEX`; `a.b=1`; `a-b=1`; `k=1/2`;
    `k=1.5x`; `# c`; `; c`; `/* open`; `k=1;`;
    whole document `x=1␍␊[A]␍␊`; whole document `[ A ]␍␊`; whole
    document `[1]␍␊`.
11. **Strict reads.** `[A]␍␊i=7␍␊d=7.5␍␊s="7"␍␊b=TRUE␍␊` with fallback
    −1 (or −1.0, "fb", false):

    | Read | Result |
    |---|---|
    | `i` as integer | 7 |
    | `d` as integer | −1 |
    | `s` as integer | −1 |
    | `b` as integer | −1 |
    | `i` as number | 7.0 |
    | `d` as number | 7.5 |
    | `s` as number | −1.0 |
    | `i` as string | "fb" |
    | `s` as string | "7" |
    | `i` as bool | false |
    | `b` as bool | true |
    | `missing` as integer | −1 |

12. **Record enumeration.** Sections `Ammo0`, `Ammo1`, `Ammo3`, each with a
    `Name` → two ammo records with IDs 0 and 1; `Ammo3` is not loaded.
13. **Links.** `[Ammo0] Name="9mm"`, `[Ammo1] Name="9MM"`,
    `[Weapon0] Name="W" AmmoName0="9mm" AmmoName1="none" AmmoName2="9Mm"
    AmmoName4="9mm"` → weapon `W` has ammo IDs [0, 0] (entry 1 dropped,
    entry 2 matches the first record, entry 4 not read because entry 3 is
    missing); default ammo ID 0. Ammo 1 is never reachable by name.
14. **Weapon defaults.** `[Ammo0] Name="a"`, `[Weapon0] Name="W"
    AmmoName0="a"` → `ShotsPerClip` 100, `MinPerturb` 100, `MaxPerturb`
    100, `DuckPerturb` 100, `ZoomedMinPerturb` 1, `ZoomedMaxPerturb` 500,
    `Range` 3000, `VectorsPerRound` 1, `FireDelay` 30, `RapidFireCount` 1,
    `MovePenalty` 0.01, `ZoomedMovePenalty` 0.7, `ZoomTime` 0.5,
    `DamageRFacter` 0.1, `MaxCamRecoilPitch` 10.0, `BaseCamRecoilPitch`
    1.2, `AniType` 5, `ClientWeaponType` 1, `Guntype` 0, `DropByPlayerDead`
    true, band lists empty.
15. **Ammo defaults.** `[Ammo0] Name="a"` → `InstDamage` 0,
    `Alt_InstDamage` 0, `InstDamageType` −1, `Priority` −1.0, `DecDelay`
    150, `JitterDecDelay` 600, `JitterWaitRecover` 0.1, `JitterRecover`
    0.3, `DamageMovePenaltyDuration` −1.0, all perturb values 0.0.
16. **Alt damage fallback.** `[Ammo0] Name="a" InstDamage=35` →
    `Alt_InstDamage` 35. Adding `Alt_InstDamage=75` → 75.
17. **Decimals in integer fields.** `[Weapon0] Name="W" MinPerturb=3
    DuckPerturb=6.5 ZoomedMaxPerturb=19.5 FireDelay=0.9 ShotsPerClip=30.0`
    → `MinPerturb` 3, `DuckPerturb` 100, `ZoomedMaxPerturb` 500,
    `FireDelay` 30, `ShotsPerClip` 100.
18. **Integers in number fields.** `[Weapon0] Name="W" MovePenalty=0
    MaxCamRecoilPitch=5` → `MovePenalty` 0.0, `MaxCamRecoilPitch` 5.0.
19. **Fire delay clamp.** `FireDelay=-20` → 0.
20. **Bool fields.** `DropByPlayerDead=FALSE` → false; `DropByPlayerDead=0`
    → true; `Hidden=true` → true.
21. **Range bands.** Weapon `Effectrange0="2500" Effectrange1="3000"
    Effectrange2="3500.9"`, ammo `InstDamage=35 Rangedamage0="1.0"
    Rangedamage1="0.7" Rangedamage2="0.35"`; band limits are 2500, 3000,
    3500. Damage before random and zone scaling, by distance:

    | Distance | Multiplier | Damage |
    |---|---|---|
    | 0 | 1.0 | 35 |
    | 2500 | 1.0 | 35 |
    | 2500.5 | 0.7 | 24.5 |
    | 3000 | 0.7 | 24.5 |
    | 3200 | 0.35 | 12.25 |
    | 3500 | 0.35 | 12.25 |
    | 3500.5 | 1.0 | 35 |

22. **Unquoted band element.** Same as 21 but `Effectrange0=2500` (no
    quotes) → limits 0, 3000, 3500; at distance 100 the multiplier is
    0.7 (damage 24.5); at distance 0 it is 1.0.
23. **Short band list.** Same as 21 but the key `Effectrange1` is missing →
    the list has one element; damage at 3200 is 35 (no range scaling).
24. **Weapon order.** `[WeaponOrder] Class0="Primary" Class1="Melee"
    Primary0="W" Melee0="K"` with weapons `W` (ID 0) and `K` (ID 1) →
    order [0, 1]. Changing `Melee0` to `"Nope"` → loading fails.
25. **Long string.** A weapon whose `Name` is 128 ASCII bytes → name is
    empty. With 127 bytes → the full name.
26. **Missing record kinds.** A weapons file with `Weapon0` but no
    `Ammo0` → loading fails. With `Ammo0` but no `Weapon0` → loading fails.
27. **Body type.** `[FigureType0] Name="Male" DashSpeed=1.0 Maxstamina=100
    Speed=90.7`, `[FigureType1] Name="Female"` → type 0: `DashSpeed` 1.0,
    `Maxstamina` 100, `Speed` 90 (truncated); type 1: `DashSpeed` 1.4,
    `Sex` female, `VariableJump` 0.0.
28. **Server player.** `[Player] WalkSpeed=90 RunSpeed=240.5
    JumpSpeed="485"` → walk 90.0, run 240.5, jump 0.0 (string rejected).
29. **Override merge (our loader).** Our TOML has weapon `w` with
    `shots_per_clip` 25 and `range` 4000. The install has
    `[Weapon0] Name="W" ShotsPerClip=30` → merged `W`: clip 30, range 3000
    (the built-in default, because the install's record lacks `Range`).
    If the install document is rejected, `w` keeps 25 and 4000.

## Open questions

1. **Steam build differences.** All field semantics come from an older
   client. The Steam weapons file is 45% larger than the US reference
   one; new keys may exist, and the Steam client may read some keys
   differently. Check: once the V1 key works, run this spec's reader over
   the Steam weapons file and list keys that appear in no table here.
2. **Base speeds.** The server reads `[Player]` `WalkSpeed`, `RunSpeed`,
   `JumpSpeed` and `LadderSpeed`, but the client's movement uses its own
   built-in speeds (70/285/330/50), and no path from the server values to
   the client mover was found. Which ones the live game uses belongs to
   `movement.md`; measure walk and run speed in the live game.
3. **`DamageMovePenalty` unit.** Values like 20, 40, 45 with durations of
   0.3 s. Whether it is a percentage of speed or an absolute speed loss is
   not settled here; `movement.md` or `weapon_firing.md` should decide.
4. **`ProgDamage` unit.** Probably hit points per second for
   `ProgDamageDuration` seconds; unconfirmed.
5. **Skills file.** One shipped file in the folder contains a string with
   a raw line break inside quotes, which this grammar rejects. Either the
   game reads it with a different reader or that file fails silently.
   Not needed for the slice.
6. **Event overwrites in the Steam build.** Whether live servers send
   text-change overwrites for weapon files is unknown; if they do, the
   live values can differ from the archive.
