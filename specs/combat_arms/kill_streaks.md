# Combat Arms: kill streaks and special-kill callouts

Source basis: reference material the user pointed the spec session to: the client's handling of the kill message (who killed whom, the multi-kill and first-kill fields, hit location, revenge and lucky-shot flags), the client's centre-screen effect banner (its animation, replacement rule and sound helper), the client UI image's scaling, the client's interface-sound path and the server's per-player match statistics. The client panel that turns a kill into a particular banner, side-log line and voice (it decides the step from the count) is **not** in the reference; where this spec says which banner or sound a kill gets, that is worked out from the install's data and the call sites around that panel, and is marked "(inferred)". Data was read from the Steam install on the Linux dev box through the repo's own archive reader: the HUD layout attributes (`attributes/layout.txt`, `[HUDLayout0]`, `[RevengeSystem]` and the weapon-effect sections), the client attributes (`attributes/clientbutes.txt`, `[Interface]` and the weapon-effect sections), the weapon list (`attributes/weapons.txt`, `HUDEffectOverride`), the main HUD layout (`xml/hud_main.xml`, control `Effect`), the English string table, the archives' file lists and the sound files' headers (lengths only).
Status: draft

Sections: Summary; Units and conventions; Part A, the rules (A1 what the server counts and sends, A2 which event a kill raises, A3 who sees and hears what, A4 the banner player, A5 the banner animation, A6 the voice player, A7 the side log, A8 weapon skins, A9 options); Part B, the Combat Arms pack (B1 steps, B2 special kills, B3 banner layout, B4 timings, B5 voice files, B6 side log, B7 weapon skins, B8 unused art); Per-frame order; Edge cases; Quirks; Test cases; Open questions.

---

## Summary

- The server counts each player's **multi-kill**: enemy kills each less than 5 s after the previous one. Every kill message carries that count, a first-kill flag, the hit location (head, groin, ...), and revenge and lucky-shot flags. Every client receives every kill message.
- The **killer's own client** shows a big animated banner at the top centre of the screen (DOUBLE KILL, MULTI KILL, ULTRA KILL, FANTASTIC, UNBELIEVABLE, UNBELIEVABLE+; also FIRST KILL, HEADSHOT, NUTSHOT, REVENGE, LUCKY SHOT) and plays an announcer voice. The banner slams in from 5x size, throws off a growing "echo" copy while it shakes, holds, then blows up and fades. The whole thing lasts 1.4 s.
- Only one banner shows at a time. Each banner kind has a **priority level**: a new banner replaces the current one unless the current one has a higher level, in which case the new one is dropped.
- A weapon can carry a **skin** (a named section of data) that swaps the banner art and some voice files for kills made with it.

The split asked for by the engine plan: **Part A** is rules any game's announcer would share (counting, signalling, banner replacement, animation shape, sound playback). **Part B** is Combat Arms' data: step names and thresholds, art, sounds, layout, timings, levels and skins. An implementation can turn all of Part B into an "announcer pack" file; nothing in Part A names a Combat Arms file or number except as a pack value.

## Units and conventions

- Time in seconds (s) or milliseconds (ms) as marked. Server times are server clock; banner times are the client's frame time.
- Screen positions are in the 1024 x 768 reference frame of `ui_system.md`: x and widths are multiplied by W/1024, y and heights by H/768 for a W x H screen. Nothing is anchored to an edge.
- Alpha 0..255. "Scale" is a size multiplier applied about the centre of the image's rectangle.
- Pack keys in `code` are the install's own data keys (players' installs carry them), quoted so an implementation can read the stock data directly.

## Part A: the rules

### A1. What the server counts and sends

**Multi-kill count** (per player, per match). Already fixed in `gear_perks.md` (Constants, Corrections 10); repeated here because the announcer reads it:

- An enemy kill made less than `window` after the same player's previous enemy kill raises the count by 1. Otherwise the count restarts at 1.
- Exactly `window` after the previous kill restarts it (strictly less than).
- Kills of teammates and of oneself neither raise nor reset it.
- `window` is a pack value (Combat Arms: 5000 ms, B1).
- Whether the killer's own death resets it was not found (Open question 2). The reference gives no reason to think it does: the count is about time between kills.

**Other per-kill facts** the server works out:

- **First kill**: a flag (one byte, nonzero = set) on the kill message when this kill is the first one the server has marked as "first kill" (Open question 3 for its scope, round or match).
- **Hit location**: of the killing hit. Head for a headshot, groin ("nut") for a groin shot. The server downgrades head to torso for damage types that cannot headshot (area, over-time and thrown-weapon damage; `death_killcam.md` 11).
- **Revenge**: set when the victim is the killer's revenge target (the player who had killed them repeatedly; `death_killcam.md` 11, revenge fields).
- **Lucky shot**: a flag; the server rolls it with a rate it receives from the lobby (Open question 6).

**The kill message.** One message per death, sent reliably to **every** client: victim, killer (255 = none), multi-kill count (one byte), weapon id, hit location, first-kill flag, revenge flags, lucky-shot flag and the other fields in `death_killcam.md` 11. There is no separate "streak" message: every announcement is worked out on the clients from this one message.

**Match statistics** (server, per player, sent to the account at the match's end; not shown live): number of first kills, headshot kills, groin-shot kills, groin-shot deaths, and one counter per streak step (double, multi, ultra, fantastic, unbelievable, unbelievable+). Separately, a **kill streak** (enemy kills without dying, no time limit) and the longest one in the match are kept. That is a different number from the multi-kill count. It is logged at the match's end and announced nowhere. The client also keeps, per player, one counter per multi-kill count value it has seen in kill messages (for the scoreboard; Open question 8).

### A2. Which event a kill raises

On every kill message that names a killer, each client works out **one centre event** for that kill, in this order (inferred; Open question 1):

1. **First kill** flag set → the first-kill event. The streak step is ignored for this kill (the count is treated as 0).
2. Otherwise the **step** for the multi-kill count n: the pack's step table maps n to a step (B1). A count with no step (n = 1 in Combat Arms) raises no step event.
3. **Special shot**: a head hit location gives the headshot event, a groin hit location the groin-shot ("nutshot") event. These are raised on the same kill as a step. Which banner wins is decided by the levels (A4). Because their level is higher than a step's, a headshot double kill shows HEADSHOT (Open question 1).
4. **Revenge** flag → the revenge event.
5. **Lucky shot** flag → the lucky-shot event (A3: the killer only). Everyone also gets an extra kill-feed line.

In co-operative mission modes (kills of AI enemies), the same banner player shows "N KILLS" with N in digits, or "COMBO" with a number when the server's co-op combo count is above 1, instead of the steps. Those numbers come from the co-op kill message, not from the multi-kill count. They are pack data (B2) and outside this spec's main scope.

### A3. Who sees and hears what

| What | Who | Notes |
|---|---|---|
| Kill-feed line with the headshot / groin-shot / revenge badges | everyone | `hud_crosshair.md` D5. The headshot badge whenever the location is head, killer or not |
| Lucky-shot extra kill-feed line | everyone | a second line for the same kill, showing only the lucky-shot icon (no other badges) |
| Centre banner (A4, A5) | the **killer's own client** only (inferred from the "killer is me" input the banner panel receives; Open question 1) | |
| Announcer voice (A6) | the killer only, local, no position | |
| Side-log line (A7) | everyone (inferred; Open question 4) | small bar on the left edge, "name STEP" |
| Killer's own "enemy kill" grunt | the killer only | the character's voice, not the announcer: `effects_sounds.md` Corrections 8. Not on a groin-shot kill |
| Victim's hurt sounds (head: `HEADSHOT.WAV` grunt, groin: `NutPain.wav`) | everyone near, 3D | `effects_sounds.md` 11. They are not announcer lines |

The headshot and groin-shot flags the banner panel uses are only updated when the local player is the killer (the client compares the killer's name with its own). For anyone else's kill they keep their last value (Quirk 3).

### A4. The banner player

There is one banner area. It holds a small pool of banner slots, but at most one is visible.

A **request** carries: the banner image, the banner's **level** (an integer; higher wins), an optional number (drawn in digits), an optional top image (drawn over the banner, e.g. the skull over HEADSHOT) and optional custom timings (A5).

On a request with level ℓ:

1. Let c be the level of the banner now visible (0 if none).
2. If c > ℓ: **drop the request**. Nothing is shown, nothing is queued, and the visible banner keeps playing. Equal levels replace.
3. Otherwise hide every banner at once (no fade-out) and start the new one at t = 0. If the same image was shown before, its slot is reused and restarted. Otherwise any hidden slot is reused, else a new slot is made. The started slot is moved to the top of the draw order.
4. Set the banner image. Hide all digits and the top image, then:
   - number N > 0: the ones digit always; the tens digit when tens or hundreds is nonzero; the hundreds digit when hundreds is nonzero. N ≥ 1000 shows only its last three digits.
   - top image given: show it.

There is **no queue**. A request that loses to a higher level is gone, even if that banner ends 0.1 s later. A step that arrives while a lower or equal banner plays cuts it off at once.

**Sound is separate.** The voice of a kill (A6) is played by the code that raised the event, whether or not its banner won (inferred; Open question 5).

On entering a world (map load, reconnect) every banner is hidden and its timer stopped.

A second, independent **text line** shares the area: a centred text (used by VIP Elimination for "+N Points"). It has its own single slot and the same timing as a default banner. Banner levels do not apply to it.

### A5. The banner animation

Each banner is drawn twice: the **main** copy and an **echo** copy of the same image (and the same digits and top image), the echo on top. Times are from the request (t = 0). Every time below is a pack value (B4):

| Symbol | Meaning |
|---|---|
| T | total time. The banner is hidden on the first frame with t ≥ T |
| t₀ | start of the slam-in |
| c | slam-in length |
| b | echo ("blow") length; starts when the slam-in ends |
| tₑ | start of the blow-out |
| d | blow-out length |
| S₀ | main copy's scale at the start of the slam-in |
| B | echo's final scale |
| L | main copy's final scale at the end of the blow-out |

**Main copy:**

- **Slam-in**, for τ = t − t₀ in [0, c): scale shrinks with an ease-in quartic,
  ```math
  s = S_0 - (S_0 - 1)\left(\frac{\tau}{c}\right)^4
  ```
  and alpha follows the scale, transparent at S₀ and opaque at 1:
  ```math
  a = \operatorname{clamp}\left(255 - \frac{(s-1)\,255}{S_0-1},\ 0,\ 255\right)
  ```
  (truncated to an integer).
- **Hold**: scale 1, alpha 255.
- **Blow-out**, for τ = t − tₑ in [0, d): scale grows with an ease-out quartic,
  ```math
  s = L - (L - 1)\left(\frac{d-\tau}{d}\right)^4
  ```
  with alpha 255 at scale 1 falling linearly to 0 at scale L (same formula as above with L in place of S₀).
- If the slam-in and the blow-out overlap (never with the stock numbers), the blow-out wins.

**Echo copy**: alpha 0 (invisible) except during the echo phase, τ = time since the slam-in ended, in [0, b): scale grows ease-out from 1 to B,
```math
s = B - (B-1)\left(\frac{b-\tau}{b}\right)^4
```
with alpha 255 at scale 1 falling linearly to 0 at scale B.

**Shake**: during the echo phase, each frame the **main** copy (with its digits and top image) is offset by a random whole number of pixels in each axis, uniformly −5..+5 (11 values). These are real screen pixels, not reference units. The echo copy does not shake. Outside the echo phase the offset is 0.

**Scaling**: every image is scaled about the centre of its own layout rectangle (after the screen stretch), so the banner grows and shrinks in place. Digits and the top image use the same scale and alpha as the copy they belong to.

**Phase starts are frame-driven**: a phase begins on the first frame at or after its start time, and the echo begins on the first frame after the slam-in timer is seen expired, so each start can be up to one frame late (Quirk 1).

**Custom timings**: a request may carry its own (T, tₑ, d). The other values stay the pack's. VIP Escort and some co-op banners use T = 3 s with d = 3 − (t₀ + c + b) (Quirk 4).

### A6. The voice player

- Each event's voice is one sound file from the pack (B5), chosen at the moment of the event. A skin (A8) may replace it.
- Played as an **interface sound**: local (no position, no distance falloff), never passed through the map's sound filter (no reverb), medium priority, at the effects volume (`music_ambience.md` 8: effects master s × 1). Nothing plays when sound is disabled (`soundenable` 0).
- **Overlap**: a new voice does **not** stop or wait for the previous one. Two kills 0.3 s apart play both lines, overlapping. (The helper has a "wait until the previous is done" mode; the announcer does not use it as far as the reference shows. Open question 5.)
- Voices are not queued and have no cooldown.
- Some events have no voice in the stock pack (revenge, lucky shot; B5).

### A7. The side log

A short line per announced event on the left side, ("NAME  DOUBLE KILL" on a coloured bar), stacked, each fading out after a few seconds. The reference does not include its logic; the pack's layout values (B6) are all there is. Treat its exact behaviour (stacking, colours of the name, which events) as open (Open question 4).

### A8. Weapon skins

- A weapon may name a **skin** (`HUDEffectOverride` in the weapon list: the name of a data section).
- When a kill message is handled, the client first selects the killing weapon's skin (the kill message's weapon id), handles the kill, then clears it.
- While a skin is selected, every banner image and voice file is looked up first in the skin's section (the layout file's section for images, the client attribute file's section for sounds). A key the section lacks falls back to the default (inferred; Open question 7).
- So the skin follows the **killer's weapon**, on the killer's client.

### A9. Options and modes

- There is **no option** to turn the announcer, the banners or the voices off. The only controls are the general effects sound switch and volume (`options.md`), which mute every interface sound.
- The `MessageDur` HUD message multiplier (`options.md`) does not touch these timings.
- The counting rule is the same in every mode that has enemy kills. Kills during warm-up add nothing to the score (`game_modes.md` S7), but the kill message still carries a warm-up flag. Whether warm-up kills are announced was not found (Open question 9).

## Part B: the Combat Arms pack

### B1. Streak steps

| Count n | Step | Banner (`[HUDLayout0]` key → file) | Voice (`[Interface]` key → file) | Side-log label (string key) | Side-log bar |
|---|---|---|---|---|---|
| 1 | none (unless first kill) | – | – | – | – |
| 2 | Double Kill | `MKComboDoubleKillImg` → `UI_HUD_Message\Effect\DoubleKill.dtx` | `DoubleKillSound` → `UI_SND\doublekill.wav` (1.41 s) | `MKDoubleKillImg` "DOUBLE KILL" | `bg_kill03.dtx` |
| 3 | Multi Kill | `MKComboMultiKillImg` → `...\MultiKill.dtx` | `MultiKillSound` → `UI_SND\multikill.wav` (1.47 s) | "MULTI KILL" | `bg_kill02.dtx` |
| 4 | Ultra Kill | `MKComboUltraKillImg` → `...\UltraKill.dtx` | `UltraKillSound` → `UI_SND\ultrakill.wav` (1.40 s) | "ULTRA KILL" | `bg_kill02.dtx` |
| 5 | Fantastic | `MKComboFantasticKillImg` → `...\fantastic.dtx` | `FantasticKillSound` → `UI_SND\fantastic.wav` (1.67 s) | "FANTASTIC" | `bg_kill02.dtx` |
| 6 | Unbelievable | `MKComboUnbelImg` → `...\Unbelievable.dtx` | `UnbelievableSound` → `UI_SND\Unbelievable.wav` (1.81 s) | "UNBELIEVABLE" | `bg_kill01.dtx` |
| ≥ 7 | Unbelievable+ | `MKComboUnbelPlusImg` → `...\Unbelievable_plus.dtx` | `UnbelievablePlusSound` → `UI_SND\Unbelievable.wav` (the same file as Unbelievable) | "UNBELIEVABLE+" | `bg_kill01.dtx` |

- Names and order are certain: they are the order of the server's six step counters, the layout keys, the string table and the art. The **thresholds** (2 to 7) are inferred from that order (Open question 1). The string table's old "Triple kill" entry is not used by any of this.
- Window: 5000 ms, strictly less (A1).
- The step art is drawn without a number.

### B2. Special kills

| Event | Banner image | Top image | Voice | Level (B4) |
|---|---|---|---|---|
| First kill | `MKComboFirstKillImg` → `Effect\FirstKill.dtx` | – | `FirstKillSound` → `UI_SND\FirstKill.wav` (1.30 s) | step level (inferred) |
| Headshot | `MKComboHeadShotImg` → `Effect\HeadShot.dtx` | `MKComboSkullImg` → `Effect\Head.dtx` (skull) | `HeatShotKillSound` (sic) → `UI_SND\headshot.wav` (0.93 s) | special shot |
| Groin shot ("nutshot") | `MKComboNutShotImg` → `Effect\NutShot.dtx` | `MKComboNutsImg` → `Effect\Nut.dtx` | `NutShotMaleKillSound` → `UI_SND\NutShot_Male.wav` (1.87 s) or `NutShotFeMaleKillSound` → `UI_SND\NutShot_Female.wav` (1.57 s), by the **victim's** body gender (inferred; Open question 10) | special shot |
| Revenge | `MKRevengeImg` → `Effect\Revenge.dtx`; the revenge panel's own copy `RevengeSuccessMsgImg` (same file, 250 x 101 at (512, 213), same timings as B4) | – | none in the default pack (`RevengeSound` exists only in the PSY skin) | revenge |
| Lucky shot | `LuckyShotEffectEXImg` → `effect\LuckyShot_EXPass.dtx` (the plain `LuckyShotEffectImg` is never used) | – | none | lucky shot |
| VIP kill (VIP modes) | `MKVIPKillEffectImg` → `Effect\VIPKill.dtx` | – | `VipKillSount` (sic) → `UI_SND\VIP_kill.wav` | VIP escort |
| Ice crush (Snowball Fight, killer only) | `SnowballFrozenKill` → `effect\IceCrush.dtx` | – | `IceCrushSound` → `UI_SND\SnowBallFight\IceCrush.wav` | snowball kill |
| Co-op single kills | `MKComboOneKillImg` "KILL" / `MKComboOneKillsImg` "KILLS" with digits `MKComboNumberImg` + `0..9` + `.dtx` | headshot skull when a head kill | – | kills |
| Co-op combo | `MKComboComboImg` → `Effect\Combo.DTX` with digits | – | – | combo |

No knife, grenade or other weapon-class callout exists. The only other kill-type marks are kill-feed icons (`hud_crosshair.md` D5).

### B3. Banner layout (`xml/hud_main.xml`, window `Effect`, 1024 x 768 reference, full screen)

| Control | Rect (x, y, w, h) | Texture's used area | Role |
|---|---|---|---|
| `effect_Img` | 256, 76, 512, 256 | 423 x 214 | the banner (centre (512, 204): top centre of the screen) |
| `effect_head` | 256, 93, 512, 256 | 323 x 122 | top image (skull, nut) |
| `Num_Hun` | 345, 175, 64, 128 | 54 x 75 | hundreds digit |
| `Num_Ten` | 385, 175, 64, 128 | 61 x 84 | tens digit |
| `Num_One` | 425, 175, 64, 128 | 54 x 76 | ones digit |
| `SubText` | 0, 257, wraps at 1024, centred | – | the "+N Points" line: bold 40, white, black 2x shadow and stroke |

All images are drawn at full colour (no tint). The banner art files are 512 x 256 images in the base set. The skin sets mix sizes; each is drawn into the same rectangle.

### B4. Timings and levels

Default banner timings (`[HUDLayout0]`):

| Key | Value | Symbol (A5) |
|---|---|---|
| `MKComboTotalTime` | 1.4 s | T |
| `MKComboMessageStartTime` | 0.0 s | t₀ |
| `MKComboMessageCreatingTime` | 0.1 s | c |
| `MKComboMessageBlowingTime` | 0.15 s | b |
| `MKComboMessageEndTime` | 1.2 s | tₑ |
| `MKComboMessageDestroyingTime` | 0.2 s | d |
| `MKComboMessageStartSize` | 5.0 | S₀ |
| `MKComboMessageBlowSize` | 3.0 | B |
| `MKComboMessageLastSize` | 5.0 | L |

The `MKComboSkull*` values (0.0, 0.1, 0.15, 1.2, 0.05; sizes 5.0, 2.0, 2.0) are read but drive nothing: the top image follows the banner's own timing (Quirk 2).

Timeline with these values: slam-in 0 to 0.1 s (5x to 1x, fading in), echo and shake 0.1 to 0.25 s (echo 1x to 3x, fading out), hold to 1.2 s, blow-out 1.2 to 1.4 s (1x to 5x, fading out), hidden at 1.4 s.

Levels, low to high (a request is dropped while a higher one shows):

| Level | Kind |
|---|---|
| 0 | HiSec case pickup (and "nothing showing") |
| 1 | co-op kill count ("N KILLS") |
| 2 | infection |
| 3 | revenge |
| 4 | streak step / combo |
| 5 | special shot (headshot, groin shot) |
| 6 | event kill |
| 7 | mutation |
| 8 | game message (bounty, mode messages) |
| 9 | extra life |
| 10 | VIP Escort messages |
| 11 | lucky shot |
| 12–14 | co-op mission complete, coin, boss down |
| 15–16 | Snowball Fight frozen kill, safe |

The kinds come from the level table. Which kinds the step, first-kill, headshot and groin-shot events use is inferred from their names (Open question 1).

### B5. Voices

As in B1 and B2. All base-pack lines are mono 16-bit, 22050 Hz for the step and headshot lines and 44100 Hz for the groin-shot lines. The lengths are given in the tables. There is no per-gender or per-character variant of the step lines. The only gendered pair is the groin shot.

### B6. Side log (`[HUDLayout0]`, values only; behaviour open, A7)

| Key | Value |
|---|---|
| `MKPos` | (10, 180): first line's position |
| `MKSize` | (256, 32): one line |
| `MKTextureWidthBlank`, `MKTextureHeightBlank` | 10, −6 |
| `MKOnlineSize` | 32 |
| `MKShowMessageTime` | 3750 ms |
| `MKShowFirstSection`, `MKShowSecondSection`, `MKShowThirdSection` | 1000, 1375, 2375 ms |
| `MKPlusAlphaValue`, `MKMinusAlphaValue` | 140, 70 |
| `MKLargeScaleValue` | 1.1 |
| `MKFont` | 21 |
| `MKColor`, `MKEnemyColor` | (255, 255, 255), (242, 101, 33) |
| `MKDoubleKillImgColor`, `MKUnbelImgColor` | (255, 233, 172), (255, 174, 0) |
| bars | first kill and double: `Superlatives\bg_kill03.dtx`; multi, ultra, fantastic: `bg_kill02.dtx`; unbelievable, unbelievable+: `bg_kill01.dtx` |

### B7. Weapon skins

| Section | Weapons using it | Images replaced (`attributes/layout.txt`) | Voices replaced (`attributes/clientbutes.txt`) |
|---|---|---|---|
| `PSYWeaponEffect` | 9 | first kill, kill, kills, double, multi, ultra, fantastic, unbelievable, unbelievable+, headshot, combo, groin shot, skull, nut, revenge (`*_PSY.dtx`) | first kill, headshot, both groin shots (one file), revenge (`*_PSY.wav`) |
| `PhantomWeaponEffect` | 2 | first kill, double, multi, ultra, fantastic, unbelievable, headshot, combo (`*_phantom.dtx`) | none |
| `GraffitiWeaponEffect` | 1 | first kill, kill, kills, double, multi, ultra, fantastic, unbelievable, headshot, combo, groin shot, revenge (`*_Graffiti.dtx`) | none |
| `TurkeyWeaponEffect` | 3 | as Graffiti (`*_turkish.dtx`) | none |
| `TMWeaponEffect` | 3 | headshot, skull (`*_RoboSkull.dtx`) | headshot (`UI_SND\TM_Headshot.wav`) |

Two of the skin keys name files that do not exist in the install (`Kil_Graffiti.dtx`; the archive has `kill_graffiti.dtx` and `kil_turkish.dtx`). See Quirk 5.

### B8. Art in the archive that no data names

`unbelievableplus_graffiti`, `headshot_halloween`, `head_halloween`, `nutshot_christmas`, `nut_christmas`, `graffitiheadshot`, and the language variants (`*_ltur`, `*_ldeu`, `*_lita`, `*_lpol`). The game applies no per-language texture swap (`ui_system.md` R1), so these are never drawn. `MKComboIceCrushImg` names `Ice_Crush_big.dtx`, which is not in the install (Snowball uses `SnowballFrozenKill` instead).

## Per-frame order

Client, one frame:

1. Network messages, in arrival order. For each kill message: update the kill feed (everyone); select the killer's weapon skin; if the local player is the killer, update the headshot and groin-shot flags; raise the event(s) of A2, each making its banner request (A4) and playing its voice (A6); clear the skin; add the side-log line.
2. HUD update: for each visible banner, apply A5 (scale, alpha, shake) for the current t; hide those with t ≥ T.
3. Draw.

Several kill messages in one frame are handled one after another, so the last request that is not dropped is the one seen. Every voice of that frame plays.

## Edge cases

- **Kill during a higher banner**: a double kill (level 4) 0.5 s after a headshot banner (level 5) shows no banner, but its voice plays (inferred).
- **Kill during an equal or lower banner**: a triple-count kill 0.6 s after DOUBLE KILL cuts it off and starts MULTI KILL at t = 0 (equal level).
- **Counts above the table**: any n ≥ 7 is Unbelievable+, again on each further kill, with the same voice as Unbelievable.
- **No killer, suicide or team kill**: no step, first-kill or special banner for the killer (the count only counts enemy kills). The kill feed still shows a head badge.
- **Damage types that cannot headshot**: the location is torso, so no headshot event.
- **Spectating or dead**: the killer's own client gets the banner even while dead (e.g. a grenade kill after death). The banner area is drawn at every HUD level (inferred; Open question 9).
- **Map change**: every banner is hidden (A4).
- **Banner with digits**: the hundreds digit shows only for N ≥ 100, tens for N ≥ 10.

## Quirks

1. **Phase starts are a frame late.** The slam-in, echo and blow-out each start on the first frame at or after their time, and the echo only on the frame after the slam-in is seen to end. At 30 fps the echo can begin up to about 33 ms late, and the main copy sits at scale 1 for that frame. Keep (invisible at normal frame rates).
2. **Skull timings unused.** The top image has its own timing values in the data, but it follows the banner's. Keep: use the banner's.
3. **Stale special-shot flags.** The headshot and groin-shot flags the banner panel reads are only refreshed on the local player's own kills. Whatever the panel does with them for other players' kills (the side log, if it uses them) sees the local player's last kill's flags. Fix in ours: take them from each kill message.
4. **Custom-timing banners compute their blow-out from the defaults**: d = 3 − (0 + 0.1 + 0.15) = 2.75 s with tₑ unchanged at 1.2 s (when not given). Keep the formula for those modes.
5. **Missing skin art.** The Graffiti skin's single "KILL" image names a file that does not exist, so that banner would draw blank (or the engine's missing-texture look) for a co-op single kill with that weapon. Keep the lookup; an empty image is fine.
6. **Unbelievable+ has no voice of its own**: it replays the Unbelievable line. Keep (data).

## Test cases

Times in seconds unless marked. "Banner" means the local killer's centre banner.

| # | Setup | Input | Expected |
|---|---|---|---|
| 1 | Player P, no previous kill | P kills an enemy at 0 | count 1; no step banner; no step voice |
| 2 | As 1 | further enemy kills at 1.0, 2.0, 3.0, 4.0, 5.0, 6.0 | counts 2..7 → DOUBLE KILL, MULTI KILL, ULTRA KILL, FANTASTIC, UNBELIEVABLE, UNBELIEVABLE+; voices doublekill, multikill, ultrakill, fantastic, Unbelievable, Unbelievable |
| 3 | P killed an enemy at 0 | next enemy kill at exactly 5.000 | count 1; no banner |
| 4 | P killed an enemy at 0 | next enemy kill at 4.999 | count 2; DOUBLE KILL |
| 5 | P killed an enemy at 0 | team kill at 1, enemy kill at 2 | count 2 at 2 (the team kill neither counts nor resets); DOUBLE KILL at 2 |
| 6 | Count 7 at 6.0 | enemy kill at 7.0 | count 8; UNBELIEVABLE+ again |
| 7 | Any | kill message with first-kill flag, count 1 | FIRST KILL banner, `FirstKill.wav` |
| 8 | Any | head kill, count 1 | HEADSHOT banner with the skull top image; `headshot.wav` |
| 9 | Any | groin kill of a female character, count 1 | NUTSHOT banner with the nut top image; `NutShot_Female.wav`; no "enemy kill" grunt for the killer |
| 10 | Banner player idle | request level 4 at t = 0 | visible 0 ≤ t < 1.4; hidden on the first frame with t ≥ 1.4 |
| 11 | Default timings | main copy at t = 0.05 | scale 5 − 4·(0.5)⁴ = 4.75; alpha ⌊255 − 3.75·255/4⌋ = 15 |
| 12 | Default timings | main copy at t = 0.5 | scale 1, alpha 255, offset (0, 0) |
| 13 | Default timings | main copy at t = 1.3 (blow-out τ = 0.1) | scale 5 − 4·(0.5)⁴ = 4.75; alpha ⌊255 − 3.75·255/4⌋ = 15 |
| 14 | Default timings | echo at 0.075 s into the echo phase | scale 3 − 2·(0.5)⁴ = 2.875; alpha ⌊255 − 1.875·255/2⌋ = 15 |
| 15 | Default timings | echo outside its phase (t = 0.05 or t = 0.5) | alpha 0 |
| 16 | Default timings, echo phase | main copy offset | each axis an integer in −5..5, new every frame; echo offset 0 |
| 17 | HEADSHOT (level 5) shown at 0 | DOUBLE KILL request (level 4) at 0.5 | request dropped; HEADSHOT keeps playing to 1.4; `doublekill.wav` still plays at 0.5 |
| 18 | DOUBLE KILL (4) shown at 0 | MULTI KILL request (4) at 0.6 | DOUBLE KILL hidden at once; MULTI KILL at its t = 0 |
| 19 | REVENGE (3) shown at 0 | DOUBLE KILL request (4) at 0.2 | REVENGE replaced |
| 20 | DOUBLE KILL shown | LUCKY SHOT (11) at 0.3 | replaced; no lucky-shot voice |
| 21 | Two kills in one frame raising DOUBLE then MULTI | – | MULTI KILL shown; both voices play, overlapping |
| 22 | Voice playing (`fantastic.wav`, 1.67 s) | next step at +0.5 | `Unbelievable.wav` starts at +0.5 while `fantastic.wav` keeps playing |
| 23 | `soundenable` 0 | any announced kill | banner as usual, no voice |
| 24 | Screen 1920 x 1080 | banner at t = 0.5 | `effect_Img` rect x 480, y 106.875, w 960, h 360 (512·1.875, 256·1.40625); centre (960, 286.875) |
| 25 | Screen 1024 x 768, t = 0.05 | `effect_Img` drawn rect | 2432 x 1216 (512·4.75 by 256·4.75), centred on (512, 204) |
| 26 | Request with number 5 | – | only `Num_One` shown, `Number_5.dtx` |
| 27 | Request with number 120 | – | hundreds 1, tens 2, ones 0 all shown |
| 28 | Request with number 1234 | – | shows 2, 3, 4 |
| 29 | Kill with a weapon whose skin is `PSYWeaponEffect`, count 2 | – | `DoubleKill_PSY.dtx`; voice `doublekill.wav` (the PSY skin has no double-kill voice) |
| 30 | Head kill with a `TMWeaponEffect` weapon | – | `HeadShot_RoboSkull.dtx`, top `Head_RoboSkull.dtx`, `TM_Headshot.wav` |
| 31 | Head kill with a `PhantomWeaponEffect` weapon | – | `Headshot_phantom.dtx`, top image the default skull `Head.dtx` (no skull key in the section), `headshot.wav` |
| 32 | Another player Q gets a double kill | on P's client | no banner, no voice; kill-feed line for Q's kill (and a side-log line, Open question 4) |
| 33 | Map change while a banner shows | – | banner hidden, timer stopped |
| 34 | Lucky-shot kill by P of R | on every client | two kill-feed lines; on P's client the `LuckyShot_EXPass.dtx` banner |

## Open questions

1. **Thresholds and levels per event.** The panel that maps the count to a step, and decides which banner each event requests (and at which level, and whether a head kill on a streak step shows the step or HEADSHOT), is missing from the reference. The table B1 (2 → Double ... 7+ → Unbelievable+) and the levels in B2 are inferred from the data's order and names. Check live: three quick kills should show MULTI KILL; a headshot second kill shows which banner and plays which voices; whether other players' streaks show a banner (A3).
2. **Death and the count.** Does the killer's own death reset the multi-kill count? Check live: kill, die within 2 s, kill again from respawn within 5 s of the first kill.
3. **First kill scope.** First kill of the round or of the match, per team or overall? Check in a round mode.
4. **Side log.** Who gets a line (everyone, or only the killer), what it says, how lines stack and fade (the four timing values and two alpha steps suggest fade in, grow to 1.1x, hold, fade out over 3.75 s), and whether single kills and special shots get lines. Check live with screenshots.
5. **Voice when the banner is dropped, and overlap.** Confirm a lower-level event still plays its voice while a higher banner shows, and that two voices overlap rather than cut.
6. **Lucky-shot roll.** The rate comes from the lobby; its value and what decides a lucky shot are not in the reference.
7. **Skin fall-back.** Confirm that a key missing from a skin section falls back to the default (test case 31), rather than drawing nothing.
8. **Scoreboard step counters.** The client counts kills per multi-kill count value per player; where (if anywhere) the scoreboard shows them.
9. **Warm-up and HUD levels.** Are warm-up kills announced? Is the banner drawn while the local player is dead (death screen up) or spectating?
10. **Groin-shot voice gender.** Male or female line by the victim's body or by the killer's? Check live with a female character killed and killing.
