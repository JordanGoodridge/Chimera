# Combat Arms: music and ambience

Source basis: the reference material the user pointed this session to. That
covers the client's background-music player (the engine's streamed-music
manager with its two stream slots and cross-fade, and its master volume),
the client's menu playlist and its option screen (sound tab), the user
profile's sound settings, the places in the client that start or stop music
(loading, round start, round and match results, mode HUDs, perks, the result
screen), the server's map music trigger object and its message to clients,
the server's placed sound object and the engine's server-to-client sound
delivery, and the engine's per-sound volume handling. Checked against the
Steam install of Combat Arms: the Classic (Steam build id 25595979): the
music files on disk and in `BGM.rez` (names, sizes, formats, lengths), and
the sound objects of the 33 world files in the two unkeyed world archives
(`WORLDS2.rez`, `WORLDS3.rez`). The music lists and interface attribute
values live in keyed attribute files that this session did not decrypt, so
which track plays at which moment is stated by attribute key, with the
likely file named only where the file name makes it obvious (marked
"inferred"; Open questions 1–3). Nothing was checked against a running game.

Status: draft

Sections: Summary; Units and conventions; Constants; Behavior (1 the music
files, 2 the music player, 3 the menu playlist, 4 music by moment, 5 map
music triggers, 6 ambient map sounds, 7 announcer and stinger sounds, 8
volume channels, 9 server or client); Order of events at phase changes;
Edge cases; Quirks; Test cases; Open questions.

---

## Summary

- Music is a **client-side MP3 stream** played from loose files on disk
  (`Game\BGM\…`), not from the archives, through a player of its own with
  two slots: a track either starts at full volume at once, or fades in (and
  the previous one fades out) linearly over a given time. Its loudness is the
  music slider alone (a separate master volume); the effects slider does not
  touch it.
- In the menus (room list, room, shop and every other lobby screen) a
  **playlist** runs: the tracks ticked in the options' music list, in order
  ("rotation", the default) or by a random skip ("random"); each new track
  fades in over **10 s**, and the next one starts 10 s before the current
  one ends, cross-fading. A single ticked track loops. The playlist only runs
  while a menu screen is shown.
- In a match there is no playlist. Music is a handful of one-shot cues the
  client starts by itself: a 2 s loading sting, a mode (or mode and map)
  start track at the first real round start, end-game tension cues in a few
  modes (Quarantine's last 20 s, Seize and Secure's capture countdown, Arms
  Race's last stage), and in a few co-op and Hunted maps music the **server**
  switches through map trigger objects. Round results are short interface
  sounds, not music. The result screen is silent apart from its win or lose
  sound.
- Map ambience is the server's placed `SoundFX` objects: almost all looping,
  "ambient" (no direction), started when the map loads, heard by distance
  with the same gain rule as every 3D sound (`effects_sounds.md` 10). The
  server keeps every looping map sound for every client regardless of
  distance.

## Units and conventions

- Distance: LithTech world units (u); positions as stored (left-handed,
  +Y up), as in the other Combat Arms specs.
- Time: music fades and stream positions use the client's wall clock in
  milliseconds (not game time, not frame time); they are evaluated once per
  client frame. Server timers are as in `game_modes.md` (20 Hz).
- Volume settings are integers 0..100. Gain is 0..1 as sent to the mixer.
- "Track" means a music file; "stream slot" one of the player's two playing
  positions. Attribute keys, console variables and object properties are in
  `code font`; they are data the game exposes.

## Constants

| Name (ours) | Value | Unit | Meaning |
|---|---|---|---|
| menu_fade | 10.0 | s | Fade-in of every playlist track, and how early the next one starts |
| loading_fade | 0 | s | Loading music, mode start music, end-game cues: start at full volume |
| quarantine_mode_fade | 10.0 | s | Quarantine's mode music after loading (uses the default fade) |
| trigger_fade_default | 1.0 | s | Map music trigger `FadingTime` default |
| quarantine_end_window | 22 → 20 | s | Round time left in which Quarantine's end cue starts |
| hq_countdown | 20 | s | Seize and Secure: capture time left at or below which its cue plays |
| music_volume_default | 100 | 0..100 | `musicvolume` console default; see Open questions 6 |
| sound_volume_default | 100 | 0..100 | `soundvolume` console default; see Open questions 6 |
| slider_step | 10 | — | Music and sound slider step |
| class_weapons / speech / default | 1.0 / 1.0 / 0.75 | × | Volume class multipliers (profile defaults; Quirk 7) |
| music_driver | 44100 Hz, 16 bit | — | The music player opens its own output |
| playlist_guard | 5000 | steps | Maximum steps of the next-track search |
| sound_radius_clamp | 1 .. 65000 | u | Server clamps every 3D or ambient sound's radii to this |

## Behavior

### 1. The music files

- **Where they are played from.** The music player opens a file path on
  disk relative to the game folder; it has no access to the `.rez`
  archives. Every music path the game uses starts with `Game\` (map
  triggers add it, Section 5) or names a file under `Game\BGM\`. The Steam
  install has the folder `Game\BGM\` with **64 files** (MP3, plus seven
  short WAV voice clips): 47 at the top, 8 in `modebgm\`, and 7 + 1 + 1 in
  `Ability\InnerFocus\`, `Ability\WarCry\`, `Ability\EagleEye\`. A user
  folder `Jukebox\` next to the game folder holds the player's own MP3s
  (Section 3); it is empty in the install.
- **`BGM.rez`** is a packed copy of the same folder: **65 entries** under
  `BGM\`, the 64 files above (same names in upper case, same sizes) plus
  `BGM\BGM_HALLOWEEN.MP3` (2 707 821 bytes), which has no loose copy. Its
  title is `LithTech Resource File` (stored raw per `rez_archive.md`), yet
  its payloads are **not** the plain files: every whole 16-byte block
  differs, the trailing `size mod 16` bytes equal the loose file's, the
  payload entropy is 8.0 bits/byte and 46 of the 65 entries share their
  first 64 bytes with another. That is the fixed-IV block-chained pattern of
  the keyed archives (Open questions 4). Since the player reads loose files,
  `BGM.rez` is not needed to play music.
- **Format.** MPEG-1 Layer III, 44.1 kHz stereo, at 128, 192 or 256 kbit/s
  (one file is 16 kHz mono, below). No file carries loop points; looping is
  "start again from the beginning" (Section 2). Lengths below were measured
  on the loose files.

| File (`Game\BGM\`) | Bytes | Length (s) | Role (where known) |
|---|---|---|---|
| `BGM_LOOP.mp3` | 3 439 514 | 143.313 | menu track (inferred) |
| `UIBGM_LOOP.mp3` | 812 997 | 50.659 | menu track (inferred) |
| `WAITINGROOM_LOOP.mp3` | 847 270 | 52.801 | menu track (inferred) |
| `BGM_03.mp3` … `BGM_12.MP3` (no 01, 02, 04) | 768 754 … 3 444 110 | 48.047 … 143.334 | menu tracks (inferred) |
| `BGM_Christmas.MP3`, `BGM_Independence.mp3`, `Halloween_BGM.mp3` | 1 248 572, 1 801 046, 1 273 002 | 78.036, 73.874, 79.499 | seasonal tracks (inferred; Section 3, event substitution) |
| `Map_Loading.mp3` | 65 518 | 2.015 | loading music (`LoadingScreenBGM`, inferred) |
| `modebgm\Search&Destroy.mp3` | 608 029 | 18.969 | a mode start track (`StartBGM`, inferred) |
| `modebgm\Elimination.mp3` | 327 160 | 20.384 | a mode start track (inferred) |
| `modebgm\SpyHunt.mp3` | 309 188 | 19.260 | a mode start track (inferred) |
| `modebgm\HiredGuns.mp3` | 1 143 852 | 35.713 | a mode start track (inferred) |
| `modebgm\VIP_start.mp3` | 242 314 | 15.081 | a mode start track (inferred) |
| `modebgm\Snow_Start.mp3` | 522 765 | 21.739 | a mode start track (inferred) |
| `modebgm\weaponrace_start.mp3` | 285 364 | 17.771 | Arms Race start track (inferred) |
| `modebgm\weaponrace_last.mp3` | 576 493 | 36.031 | Arms Race last stage (`last_stage_bgm`, inferred) |
| `QRT_Count_BGM.mp3` | 434 994 | 27.123 | Quarantine end cue (`QuarantineModeEndBGM`, inferred) |
| `HQM_count_BGM.mp3` | 334 265 | 20.828 | Seize and Secure capture cue (`HQMOccupyProgressBGM`, inferred) |
| `Zombie_Ambience.mp3`, `Ambient_BGM.mp3` | 1 605 279, 1 239 147 | 100.266, 77.383 | Quarantine mode track or ambience (inferred) |
| `Hunted_DarkHill_BGM.mp3` | 7 702 468 | 320.894 | Hunted, map trigger (Section 5, read from the map) |
| `Start_BGM.mp3`, `Last_BGM.mp3`, `AmbientCosta_BGM.mp3` | 366 448, 1 537 988, 2 242 249 | 22.839, 96.060, 70.038 | co-op map triggers (read from the map) |
| `SnowStorm_Start.mp3`, `SnowStorm_Boss.mp3`, `SnowStorm_Ending.mp3` | 675 738, 2 308 963, 696 463 | 28.113, 96.031, 28.845 | co-op map triggers / ending (read from the map) |
| `Null_60S.mp3` | 1 501 457 | 602.281 | 10 min of 16 kHz mono (near silence; Open questions 3) |
| `Ability\WarCry\WarCry.mp3`, `Ability\EagleEye\Alignment.mp3` | 153 289, 142 840 | 9.517, 8.864 | perk cues (Section 4) |
| `Ability\InnerFocus\*.wav` (7) | 75 702 … 129 150 | 0.857 … 1.463 | perk voice clips (16-bit mono WAV) |

  The other loose files (`DesertFox_BGM01..03`, `NemexisHQ_BGM01..03`,
  `NemexisLAB_BGM01..03`, `NemexisLAB_BGMDEARD`, `SH01_Intro`,
  `SH01_BGM01..03`, `Ship_Start`, `Ship_Play`, `Ship_Last`,
  `MineShaftStart_BGM`, `MineShaftLast_BGM`) are by name co-op or event map
  music; none is used by a map in the two unkeyed world archives.

### 2. The music player

One player per client, separate from the sound-effect mixer, with its own
output and master volume (Section 8). It has two stream slots, **primary**
and **secondary**, a "last started" file name, a change time t₀ (wall clock)
and a fade time T (ms).

**Start a track** (file F, loop flag, fade T):

1. If F equals the last started name (compared without case) the request is
   ignored. A missing file name is ignored.
2. Slot choice:
   - primary empty, **or** T = 0: F goes to the primary slot; the secondary
     slot is closed. With T = 0 the track starts at gain 1, else at 0.
     (If the primary already held a stream it is **not** closed; Quirk 3.)
   - else, a cross-fade is already running (secondary occupied): the
     secondary moves to primary (the stream that was fading out is closed at
     once), and F goes to the secondary slot at gain 0.
   - else: F goes to the secondary slot at gain 0 (a cross-fade).
3. The file is opened. If it cannot be opened, nothing else happens (the
   slot changes of step 2 have already been made).
4. t₀ ← now, T ← the requested fade. Loop: the stream repeats forever from
   the start; no loop: it plays once and then stays silent **while still
   occupying the slot** (the player considers it "playing" until stopped).
   The last started name ← F.

**Stop:** both slots are closed, T ← 0, the last started name is cleared.

**Every client frame** (before the frame's sound update; also while the
game window is not focused), with e = now − t₀ in ms:

```math
\text{if } e < T:\quad g_{new} = e/T,\qquad g_{old} = 1 - e/T
```

- no secondary stream: the primary gets g_new (a fade-in);
- secondary present: the primary (old) gets g_old, the secondary (new) gets
  g_new (a cross-fade, linear in the mixer's volume value; Open questions 5).
- e ≥ T and a secondary exists: the secondary becomes the primary at gain 1
  (the old primary is closed). e ≥ T, primary only, gain below 0.99: gain 1.

**"Near the end"**, used by the playlist: true when the primary is the only
stream and its remaining time in the current pass, total length − position
(ms), is below T. A track started with T = 0 is never near its end.

### 3. The menu playlist

**The list.** Built-in tracks come from the attribute file
`BGM_Setting.txt` (in `ATTRIBUTES.rez`): a version string, a list of
entries with a display name and a file path, and a set of default ticks.
An entry whose file does not exist on disk is shown greyed out and can never
play. After them come the player's own MP3s: every `*.mp3` in the
`Jukebox\` folder (file name without extension as the display name), ticked
by default when first seen, playable only if the player can open it. The
ticks are saved per user in `Game\Etc\BGMUserProfile.txt` (an encrypted
attribute file: version, one tick per built-in entry, name and tick per
Jukebox file); when the saved version differs from the list's version the
saved ticks are thrown away and the defaults used. Jukebox files that have
gone are dropped from the saved list; new ones are added.

**Eligible** = ticked and playable. N = number of eligible entries (built-in
first, then Jukebox, as one list of length L). A cursor c starts at −1.

**Next track** (only when N ≥ 1; with N = 0 there is no menu music):

1. loop ← (N = 1).
2. k ← 0 in rotation mode or when N = 1; in random mode k ← uniform integer
   in 0..N−1.
3. Step c ← (c + 1) mod L until an eligible entry has been passed k times
   and the next eligible one is reached; that entry is the track (at most
   5000 steps). A Jukebox entry plays `Jukebox\<name>.mp3`.

So rotation plays the list in order and wraps; random skips 0..N−1 eligible
tracks forward from the current one, which **can land on the current track
again** (k = N − 1), Quirk 4.

**The menu update**, every client frame while a menu screen is shown (the
"screen state": the lobby, room list, room, shop, inventory, options and
result screens; not the loading screen and not a match):

1. If music is paused (the result screen sets this, Section 4) → stop.
2. Else, if a stream is loaded:
   - no current screen → stop;
   - the cursor's entry is eligible: if the track is near its end, start the
     next track with loop as computed and **T = 10 s** (a 10 s cross-fade
     that starts 10 s before the end);
   - the cursor's entry is not eligible (it was unticked in the options, or
     the cursor is −1 because the loaded track is not a playlist track, such
     as the loading or mode music) → stop, then start the next track with
     T = 10 s.
3. Else (nothing loaded) and a screen is shown → start the next track with
   T = 10 s (a fade-in from silence).

Every start goes through a "current music" name kept by the menu layer: a
request for the name already current is ignored; stop clears it. After a
start the music master volume is set again from the settings (Section 8).

**Rotation or random** is the option tab's switch (`bgmrotation`, 1 =
rotation). The playlist itself starts in rotation mode at every launch and
only takes the saved choice when the player opens the options' sound tab
(Quirk 2).

**Seasonal substitution.** A client event file (`clientevent.txt`) can
map a sound path to another between a start and an end date and time; when
any such mapping is active, every music start (menu, loading, mode, map
trigger) is looked up there first (exact path, case-sensitive) and the
replacement is played instead.

### 4. Music by moment

All starts below are by the client alone unless stated. "Interface sound"
means a 2D sound at full gain through the effects channel
(`effects_sounds.md` 10). Attribute keys are in the client interface
attributes (`clientbutes.txt`) or the room data (`gameroom.txt`).

| Moment | What plays | Loop | Fade | Stops / ends |
|---|---|---|---|---|
| Lobby screens (room list, room, shop, …) | menu playlist (Section 3) | 1 track: yes | 10 s in, 10 s cross-fades | any start below; leaving the screen state |
| Room countdown (5 s) | countdown interface sound each second (`lobby.md` S) | — | — | — |
| Going to the game server (room START, host, single play, tutorial) | stop, then `LoadingScreenBGM` (`Map_Loading.mp3`, 2.0 s, inferred) | no | 0 | it ends after 2 s and stays loaded but silent |
| Loading screen | the loading track above; once loading reports finished, music is stopped every loading-screen frame | — | — | — |
| Loading screen closes, Quarantine only | `QuarantineModeBGM` | no | **10 s** fade-in | ends on its own |
| Warm-up round start (alive) | interface sound `FirstSpawnSound` | — | — | — |
| First real round start after entering the map (alive) | interface sound `RoundStartSound`, then **stop and start the mode music**: the room data's `StartBGM_<map index>` for this mode if present, else the mode's `StartBGM`; nothing if empty | the mode's `LoopBGM` | 0 | stays (looping) or ends; later rounds do not restart it |
| Later real round starts | `RoundStartSound` only | — | — | — |
| Quarantine, real round, round time left in (20, 22] s | stop, `QuarantineModeEndBGM` (once per round) | no | 0 | ends on its own |
| Seize and Secure, capture timer at or below 20 s | stop, `HQMOccupyProgressBGM` | no | 0 | stopped (silence) when the capture timer stops or rises above 20 s, or the round timer runs out |
| Arms Race, a player reaches the last stage (server event) | interface sound `last_stage_sound`; stop, `last_stage_bgm`, unless already playing | **yes** | 0 | when that player leaves and no one else is on the last stage, or all players are removed: stop, and restart the mode's start music if the mode's `LoopBGM` is set |
| Perk with the war-cry or eagle-eye cue activated (`gear_perks.md`) | stop, the perk's sound file as music (`Ability\WarCry\WarCry.mp3`, `Ability\EagleEye\Alignment.mp3`) | no | 0 | ends on its own; the mode music does **not** come back (Quirk 5) |
| Map music trigger (server) | Section 5 | per object | per object | per object |
| Round result (each round, also the warm-up) | team modes: interface sound `RoundAlphaTeamWinSound` or `RoundBravoTeamWinSound` by the **winning team** (the same for both teams); Quarantine: `QRTHumanWinSound` or `QRTInfectorWinSound`; free-for-all modes, Spy Hunt for non-Alpha players, co-op and host games: none; a draw: none | — | — | — |
| Match end → result screen | stop; music paused (silence on the whole result screen); on the first result-screen frame one interface sound: `ResultWinSound` or `ResultLoseSound` (team modes by your team; no-team modes by being first; Quarantine always "win"; co-op by success); a draw plays none | — | — | — |
| Leaving the result screen (back to the room) | music unpaused; the playlist starts its next track with a 10 s fade-in | — | — | — |
| Leaving a match from the in-game menu | stop | — | — | — |
| Cabin Fever 2 (co-op) success | its ending cinematic plays `CF2CinematicEndingBGM` | yes | 0 | when the cinematic closes |

Notes:

- During a match the menu update does not run, so nothing restarts music
  after a cue ends: once the mode music (non-looping) or a cue ends, the
  match is silent apart from map sounds.
- The mode music is chosen by the room's mode and map index; `LoopBGM` is
  per mode (a clan or override mode's own value first).
- Win and lose here are interface sounds through the effects channel and
  scale with the effects slider, not the music slider.

### 5. Map music triggers (`BGMTrigger`)

A server map object. Properties that matter: `GameType` (a game-mode name;
the object does nothing unless it equals the running mode), `FileName`
(path below `Game\`), `FadingTime` (s, default 1.0), `Loop` (default
false), `Locked`, plus the usual trigger properties (it is a trigger: map
commands send it messages).

- Message `ON`: if `FileName` is not empty and the mode matches, the object
  is marked on and every client is sent a "music on" message (reliable)
  with loop, fade time and file name. `OFF`: marked off, every client gets
  "music off".
- A client receiving "music on" applies the seasonal substitution, prefixes
  `Game\` and starts it **directly on the player** (Section 2) with the
  given loop and fade: a cross-fade from whatever plays when the fade is
  above 0, an immediate replacement when it is 0. "Music off" stops the
  player.
- **Joining late:** when a client enters the world, the server looks for the
  first trigger that is on **and** looping and sends just that one to the
  joiner. A non-looping track that is still playing is not sent.
- These starts bypass the menu layer's "current music" name, so a later
  menu or mode start of the same file is not blocked by it (only by the
  player's own last-started check).

In the 33 unkeyed maps there are 17 such objects in 7 maps: Dark Hills
(Hunted) one, `BGM\Hunted_DarkHill_BGM.mp3`, fade 1, no loop; the Snow
Storm maps three each (`BGM\SnowStorm_Start.mp3` fade 0 no loop; an ambience
`SND\SnowStorm\wind_Ambience.MP3` fade 4 looping; `BGM\SnowStorm_Boss.mp3`
fade 0 looping); Costa Recon three (`BGM\Start_BGM.mp3` fade 4 no loop;
`BGM\Last_BGM.mp3` and `BGM\AmbientCosta_BGM.mp3` fade 4 looping); the
other co-op maps three each. `SND\SnowStorm\wind_Ambience.MP3` exists only
inside `SND.rez`, not on disk, so that start fails silently (Open
questions 7).

### 6. Ambient map sounds (`SoundFX`)

How one object plays is in `effects_sounds.md` 11 ("Map sounds"); this
section adds what that spec leaves out and corrects two points (Open
questions 8, 9).

- **Properties** (defaults): `StartOn` (true), `Sound` (a file path or a
  sound-set name), `Priority` (0), `OuterRadius` (500), `InnerRadius`
  (100), `Volume` (100), `PitchShift` (1.0; inaudible, `effects_sounds.md`
  Quirk 10), `Ambient` (true), `Loop` (true), `PlayAttached` (false),
  `Filter` (`UnFiltered`). Radii are clamped to 1..65000 u by the server.
- **Ambient** (`Ambient` true): gain by distance with the formula of
  `effects_sounds.md` 10, no direction (centred). `Ambient` false: a 3D
  sound at the object.
- **Plain file**: loops if `Loop`, follows the object if `PlayAttached`,
  volume `Volume` (sent only when below 100), the object's radii, the
  object's `Filter` (a reverb preset applied only when the engine's
  environmental reverb is available and on; Open questions 10).
- **Sound-set name**: a file is drawn from the set by weight, the set's
  play chance can skip it, the object's radii are used, but `Loop` and
  `PlayAttached` are ignored (the set's own loop flag decides; set sounds
  always follow the object), and the volume is 100 when the object's
  `Volume` is 100, otherwise **the set's** volume. No stock map uses a set.
- **One-shot** (`Loop` false): the server forgets the sound after its
  length. There is no random-interval repeat in this game: a sound set's
  min and max delay fields are read but never used. Repeats only happen when
  map commands send `ON`/`TRIGGER` again (`effects_sounds.md` 11).
- **Who receives them**: an object's sound is kept by the server as a
  tracked sound and sent to **every** client, whatever the distance (the
  engine's range check for tracked sounds is disabled); a client that joins
  later receives every tracked sound still running and starts it from the
  beginning of the file (no time sync). Clients mute it by distance
  (`effects_sounds.md` 10). The inner radius travels as ⌊255·r/R⌋ in one
  byte and is rebuilt as that/255·R.
- **How many at once**: each looping, started object is one live sound on
  every client from map load, audible or not; they count towards the 255
  live sounds and 255 mixer voices (`effects_sounds.md` 10). The busiest
  unkeyed map, Dam, has 74 objects.
- **Census** of the 33 world files in `WORLDS2.rez` and `WORLDS3.rez`:
  786 `SoundFX`; 642 looping, 144 one-shot; 730 ambient, 56 3D; 585 start
  on; 41 attached; 338 with `Volume` below 100; all name `.wav` files
  directly; outer radius below 500 u: 147, 500–999: 288, 1000–1999: 174,
  2000–3999: 115, 4000 and above: 62 (31 above 6000); 45 with inner ≥
  outer; `Filter` other than `UnFiltered`: 74 (`ROOM` 53, `CONCERTHALL` 20,
  `CITY` 1). Also 13 `Filter` volumes in 3 maps and the
  `GlobalSoundFilter` world property (two maps use `PARKINGLOT` and
  `LIVINGROOM`). The world properties' `MusicControlFile` is empty in every
  map that has it (the engine's interactive music system is not used).

### 7. Announcer and stinger sounds

Combat Arms has no round-start or round-end announcer voice beyond what is
listed in Section 4 (interface stingers) and `effects_sounds.md` 11
(call-outs, hurt sounds). Mode stingers already in other specs: the bomb
planted, countdown beeps and defused sounds (`game_modes.md` M3), the
bullion banking sound (`game_modes_2.md`), the lobby countdown sound
(`lobby.md` S). Interface stingers named in the client attributes and
played by code: `MapLoadingSound`, `GameStartSound`, `GameOverSound`,
`FirstSpawnSound`, `RoundStartSound`, the round and result win/lose sounds
of Section 4, `last_stage_sound` and `weapon_change_sound` (Arms Race), the
level-up sound. Their file names are in the keyed attribute file (Open
questions 1).

### 8. Volume channels

There are two output channels, not five:

| Channel | Setting (console / profile) | Range, step | Gain it sets |
|---|---|---|---|
| Music | `musicvolume` with on/off `enablemusic` | 0..100, 10 | music master m = `musicvolume`/100, or 0 when off |
| Effects ("sound") | `soundvolume` with on/off `enablesound` | 0..100, 10 | effects master s = `soundvolume`/100, or 0 when off |

- **Music gain** of a stream = m × slot gain (Section 2). Nothing else
  (no master above it, no effects slider).
- **Effects gain** of a new sound = s × g(d) × V/100 for a 3D or ambient
  sound (`effects_sounds.md` 10), s × V/100 for a 2D sound with its own
  volume, s for an interface sound. Map ambience, weapons, voices,
  footsteps and interface sounds all share this one slider.
- **Volume classes** (weapons 1.0, speech 1.0, default 0.75 in the profile;
  no option screen changes them) are applied only to the sounds that are
  playing at the moment the sound settings are applied (Quirk 7). New sounds
  are not scaled by them.
- **Voice chat** has its own microphone and speaker sliders when the build
  has voice chat; while someone speaks or an advert plays, music and effects
  are lowered to the "adjust" volume (0 for an advert) until it ends. The
  Steam build's option screen greys these out (Open questions 11).
- **Window minimized**: both masters go to 0 while the window is iconic and
  back to the saved values when it is restored. Losing focus without
  minimizing changes nothing.
- **Applying options**: the music slider and switch act at once (live, also
  while dragging); OK saves them to the profile, Cancel restores the saved
  values. Saving also writes the playlist ticks. A launcher option can force
  music off (music volume 0) at start.
- The sliders feed `options.md` (not yet written) with these names: music
  on/off, music volume, sound on/off, sound volume, rotation/random, and the
  music list with one tick per entry.

### 9. Server or client

- **Server decides**: map music triggers (on, off, which file, loop, fade)
  and the late-join resend; map sound objects (start, stop, file, radii,
  volume, loop); every phase change the client reacts to (round start with
  its warm-up flag, round result with the winner, match end), the Arms Race
  last-stage event, and the bomb and banking events of other specs.
- **Client decides alone**: the menu playlist and its order, loading music,
  which mode track to play and when (first real round start while alive),
  the Quarantine and Seize and Secure cues (from the client's own round and
  capture clocks), perk cues, result-screen silence and sounds, every
  volume, every fade, and whether a music file exists.

## Order of events at phase changes

**Room to match (client):**

1. Countdown ends, the server address arrives (`lobby.md` S).
2. Stop music; start the loading track (full volume, no loop).
3. Switch to the loading screen. The menu update stops running; the
   loading track plays out (2 s) and stays loaded.
4. When loading reports finished: music is stopped on every loading-screen
   frame until the screen closes.
5. Loading screen closes. Quarantine: the mode music starts with a 10 s
   fade-in.
6. Round messages arrive. Warm-up start: `FirstSpawnSound` (if alive).
   First real round start: `RoundStartSound`, then stop + mode music.

**Round end (client):** round result message → scoreboard result with its
win sound → (next round start: `RoundStartSound` only). Music is not
touched at round end.

**Match end (client):**

1. Match result → the client leaves the world and switches to the result
   screen.
2. The result screen stops music, stops any round win/lose loops, sets
   music paused.
3. First result-screen frame: the result win or lose interface sound.
4. Until the screen is left, the menu update sees "paused" and keeps the
   player stopped.
5. Leaving the screen unpauses; the next menu update starts the playlist's
   next track with a 10 s fade-in.

## Edge cases

- **A round ends during a fade**: nothing in the game reacts; the fade
  completes on the wall clock. A stop (for example at match end) cuts both
  slots at once with no fade-out; there is never a fade to silence.
- **Two starts during one cross-fade**: the track that was fading out is
  cut, the track that was fading in becomes the outgoing one at whatever
  gain it had reached, and the fade restarts from 0 for the newest one.
- **Music muted** (switch off or volume 0): tracks still start, advance and
  cross-fade, silently; turning music on again resumes at the current point.
- **A missing track**: a menu entry whose file is missing is never eligible.
  A loading, mode, cue or trigger file that is missing: the start does
  nothing (the previous stream may already have been moved or closed,
  Section 2 step 2), so the result is silence or the old track. An empty
  mode `StartBGM` plays nothing and leaves the loading track loaded.
- **N = 0 playlist entries**: the menus are silent.
- **N = 1**: the track loops forever with no cross-fade; 10 s fade-in only
  at its first start.
- **A track shorter than 10 s** in the playlist is "near its end" from its
  first frame, so the next one starts at once.
- **Unticking the playing track** in the options: on the next menu frame
  it is cut and the next eligible one fades in over 10 s.
- **Returning from a match while the mode music still plays**: leaving
  stops it (in-game menu exit or result screen), so the playlist always
  starts fresh.
- **The same file twice**: a start of the file just started is ignored even
  if it has finished playing (non-looping) until something else is started
  or music is stopped.
- **Map sound with inner ≥ outer** (45 objects): the gain formula never
  applies (the listener is never between r and R), so the sound stays at
  its starting gain (1) everywhere inside R and is muted beyond R. On the
  wire ⌊255·r/R⌋ is cut to one byte, so for r > R it wraps (r = 2R sends
  254). Keep the "full gain inside R" result.
- **Ambient map sound started while you are beyond R**: muted; it is heard
  as soon as you walk inside R, at the gain of the formula.

## Quirks

1. **Silent matches.** Once the mode music (usually not looping) or a cue
   ends, a match has no music until the result screen ends. Keep.
2. **Random mode forgotten at launch.** The playlist starts in rotation
   every launch and switches to random only after the options' sound tab
   has been opened once. Keep (cheap) or fix as an option.
3. **Orphaned stream.** A zero-fade start while a track is loaded replaces
   the primary slot without closing the old stream, which keeps playing,
   unaffected by later stops, until it ends (forever if it was looping).
   Menu and match starts always stop first, so in practice only map
   triggers with fade 0 can do this (Snow Storm's `Start_BGM` then
   `BOSS_BGM`). Do not reproduce; note it.
4. **Random can repeat.** k = N − 1 lands on the current track; the start
   is ignored as "same file", the track ends, and on each following frame
   another draw is made until a different file comes up, so the next track
   starts without a cross-fade after a short gap. Keep the draw rule; the
   gap is a side effect.
5. **Perk cue ends the mode music.** Activating the war-cry or eagle-eye
   perk replaces the mode music for the rest of the match. Keep.
6. **Round win sound is per winning team, not per listener.** Both teams
   hear "Alpha wins" when Alpha wins. Keep.
7. **Volume classes only on re-apply.** Class multipliers (and a second
   copy of the effects master, ×1.27 in integer steps) are applied only to
   sounds playing when sound settings are applied, so pressing OK in the
   options can briefly make long ambient loops louder or quieter until they
   are restarted. Do not reproduce; new sounds use s × g × V/100.

## Test cases

Logic cases (an implementer without the game can check them; "gain" is
music master × slot gain unless stated):

1. **Fade-in.** Playlist with one eligible track, music 100, start at
   t = 0 → gain 0 at 0 s, 0.25 at 2.5 s, 0.5 at 5 s, 1.0 at 10 s and after;
   loop on.
2. **Cross-fade timing.** Rotation, eligible tracks A (60 s), B (50 s);
   A started at t = 0 with a 10 s fade-in → at t = 50.0 s (10 s left) B
   starts; at t = 52.5 s A gain 0.75, B gain 0.25; at t = 60 s A is closed,
   B gain 1; B's next track (A) starts at t = 50 + 40 = 90 s.
3. **Rotation order.** Built-in entries [A ticked, B unticked, C ticked],
   one Jukebox file J ticked → order A, C, J, A, …; with B's file missing
   the same order.
4. **Random draw.** Eligible A, B, C, current A, random mode → k = 0 → B,
   k = 1 → C, k = 2 → A (ignored as same file; a new draw the next frame).
5. **Music slider.** Music 60 during a cross-fade at e = T/2 → each stream
   gain 0.3. Music switch off → 0, tracks keep advancing; on again → 0.6 on
   the playing track at its current position.
6. **Room to match.** Menu music playing, room START → playlist cut at once,
   loading track at gain 1 (music 100), silent 2.0 s later; at the first
   real round start (alive) `RoundStartSound` and the mode track at full
   volume; no music at the second round start.
7. **Warm-up.** A mode with a warm-up: warm-up start → only
   `FirstSpawnSound`; mode music only at the first real round.
8. **Dead at round start.** Player dead when the first real round starts →
   no `RoundStartSound`, no mode music; it does not start later in that
   map.
9. **Quarantine end cue.** Real round, round clock passes 21.0 s left →
   music cut, `QuarantineModeEndBGM` at full volume, once; the warm-up never
   triggers it.
10. **Seize and Secure cue.** Capture timer counts 25 → 20 s → cue starts at
    20; the capture is broken at 12 s → music stops (silence).
11. **Result screen.** Match won (team mode) → music stops on entering the
    result screen, `ResultWinSound` once; no music for its whole time (up
    to 60 s, `lobby.md`); back in the room → playlist fades in over 10 s.
12. **Map trigger late join.** Server has `Last_BGM` (looping) on and
    `Start_BGM` (not looping) finished earlier; a client joins → it gets
    "music on" for `Game\BGM\Last_BGM.mp3`, loop, fade 4 s, and starts it
    with a 4 s fade-in.
13. **Map trigger wrong mode.** A `BGMTrigger` with `GameType` `HungerGame`
    in a TDM game receives `ON` → no message, no music.
14. **Ambient gain (`effects_sounds.md` 10 rule).** Ambient loop R 6000,
    r 4000 (sent as 170/255): d = 3000 → 1.0 (started there); 4200 → 0.59;
    4400 → 0.16; 4600 and beyond → 0. With effects 50 → half of each.
15. **Ambient gain, rounding.** R 2000, r 1000 (sent as 127/255 →
    996.078): d = 1200 → 0.5557; 1400 → 0.0397; zero at 1414.22.
16. **Inner ≥ outer.** Ambient loop R 500, r 600, listener 450 u away →
    gain 1; at 499 → 1; at 501 → muted.
17. **Map sounds reach everyone.** A looping `SoundFX` 20 000 u from a
    client → the client has it as a live (muted) sound; walking within R
    makes it audible without any new server message.
18. **Window minimized.** Music 80, effects 70, window minimized → both
    masters 0; restored → 0.8 and 0.7.

Install cases (coordinator's Linux box, Steam build id 25595979):

19. **`BGM.rez`**: 65 entries, all under `BGM\`; sizes include
    `BGM\BGM_LOOP.MP3` 3 439 514, `BGM\MAP_LOADING.MP3` 65 518,
    `BGM\MODEBGM\SEARCH&DESTROY.MP3` 608 029, `BGM\BGM_HALLOWEEN.MP3`
    2 707 821 (the one without a loose copy). Payload bytes differ from the
    loose files except the last `size mod 16` bytes.
20. **Loose `Game\BGM\`**: 64 files. `BGM_LOOP.mp3` 3 439 514 bytes,
    143.31 s, 192 kbit/s, 44.1 kHz stereo; `WAITINGROOM_LOOP.mp3` 847 270
    bytes, 52.80 s, 128 kbit/s; `Map_Loading.mp3` 65 518 bytes, 2.02 s;
    `Null_60S.mp3` 1 501 457 bytes, 602.28 s, 16 kHz mono;
    `HQM_count_BGM.mp3` 20.83 s; `QRT_Count_BGM.mp3` 27.12 s;
    `modebgm\weaponrace_last.mp3` 36.03 s. (Lengths ±0.05 s by decoder.)
21. **One map's ambience**: `WORLDS2.rez`, `worlds2\training.dat`
    (201 objects) has 15 `SoundFX`: 14 bird loops and one `End_BGM`.
    Decoded `Birds_Ambient`: position (−5065, 165.5, −5453), `StartOn`
    true, `Sound` `SND\Training\Birds_Ambient.wav` (in `SND.rez`, 7.995 s,
    44.1 kHz 16-bit stereo), `OuterRadius` 6000, `InnerRadius` 4000,
    `Volume` 100, `PitchShift` 1.0, `Ambient` true, `Loop` true,
    `PlayAttached` false, `Filter` `UnFiltered`, `Priority` 0. `End_BGM`:
    same position, `StartOn` false, `SND\Training\Training_BGM.wav`
    (20.782 s), outer 8000, inner 6000, ambient, not looping.
22. **Census**: the 33 world files of `WORLDS2.rez` and `WORLDS3.rez` hold
    786 `SoundFX` and 17 `BGMTrigger` (numbers in Section 6 and 5); the
    Dark Hills trigger names `BGM\Hunted_DarkHill_BGM.mp3`, `GameType`
    `HungerGame`, `FadingTime` 1.0, `Loop` false.

## Open questions

1. **Keyed data not read.** `BGM_Setting.txt`, `clientbutes.txt` (the
   interface attribute keys of Section 4 and 7), `gameroom.txt` (`StartBGM`,
   `StartBGM_<n>`, `LoopBGM` per mode) and `clientevent.txt` are in the V1
   keyed `ATTRIBUTES.rez`; this session did not decrypt them. Needed: the
   playlist entries (names, paths, default ticks, version), the value of
   every key in Section 4, each mode's start track and loop flag. Every
   "inferred" in Section 1 should be replaced by these values.
2. **Menu track list and paths.** Whether the playlist's paths start with
   `Game\BGM\` (the file-exists test is relative to the program's working
   folder) and which of `BGM_LOOP`, `UIBGM_LOOP`, `WAITINGROOM_LOOP`,
   `BGM_03`…`BGM_12` are in it and ticked by default.
3. **`Null_60S.mp3`** (10 min of near silence at 16 kHz): probably a
   "no music" entry or a mode's silent start track; check where the data
   names it.
4. **`BGM.rez` cipher.** Its title says raw, but payloads look block-cipher
   encrypted with a fixed IV per file (the pattern of the keyed archives,
   `rez_archive.md`). Which key, if any, decrypts it, and does the game read
   it at all (the music player does not)? This contradicts `rez_archive.md`
   ("`LithTech Resource File`: stored raw"); that spec's owner should note
   the exception.
5. **Fade curve.** The fade sets the mixer's sample volume linearly in
   time; whether the audio library treats that value as linear amplitude or
   as a perceptual (dB-like) scale was not checked. Listen to a 10 s
   cross-fade: equal loudness at 5 s means a perceptual scale.
6. **Default volumes.** The profile's built-in fallbacks for both sliders
   are 0, the console defaults 100, and the shipped default profiles
   (`Profiles\*_Default.prf`) are encrypted. Read the shipped profile's
   `[Sound]` section for the first-launch values.
7. **Archived trigger file.** The Snow Storm ambience trigger names
   `SND\SnowStorm\wind_Ambience.MP3`, which is only in `SND.rez`. If the
   stream player really cannot read archives, that cue is silent in the
   Steam build; check in game.
8. **Contradiction with `effects_sounds.md` 11 and 12**: it says server
   sounds reach only clients within 2R. True for one-shot server sounds
   (hurt, bounce); map `SoundFX` objects are tracked sounds, sent to every
   client at any distance (the engine's range check for them is disabled).
   That spec's owner should update section 12.
9. **Contradiction with `effects_sounds.md` 10**: "the player's master and
   per-class sliders multiply on top". There is one effects slider; the
   per-class multipliers reach only sounds already playing when settings
   are applied (Quirk 7). Also its map-sound note that a sound set "uses
   the object's own volume" holds only for volume 100.
10. **Sound filters.** `Filter` names reverb presets (`ROOM`,
    `CONCERTHALL`, `CITY`, `PARKINGLOT`, `LIVINGROOM`) that only take effect
    when the engine's environmental reverb is supported and enabled; their
    parameters are in `soundfilters.txt` (keyed). Whether the Steam build
    ever enables it, and what each preset sounds like, is open.
11. **Voice chat ducking.** Whether the Steam build has voice chat at all
    (its option controls are disabled) and so whether the music and effects
    ducking ever happens.
12. **Mode music when the mode has no `StartBGM`.** TDM and other kill modes
    may have none, in which case the loading track is the only music of the
    match. Check with the room data.
13. **`options.md`** has not landed; the slider names and ranges in
    Section 8 should be checked against it when it does, and `lobby.md`'s
    result screen against Section 4.
