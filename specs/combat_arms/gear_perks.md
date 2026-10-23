# Combat Arms: gear and perks

Source basis: reference material the user pointed the spec session to: the server's damage and armour bookkeeping (gear list, armour points, repair), the server player object's gear acquisition, health and armour reset, stamina capacity and drain, ammunition maxima and the spawn loadout, the server's perk manager and the Warlord ammo-box object, the client's perk logic and perk HUD (throw bar, buff icons), the client movement speed and penalty code, the client weapon's ammunition and spread hooks, the shared attribute managers for character models, attachments, gear, gear mods, set items, perks and items, the lobby-to-server item hand-over and the respawn request check. Data was read from a US-region attribute data set (`WEAPONS.TXT`, `MODELBUTES.TXT`, `ATTACHMENTS.TXT`, `MOD.TXT`, `PERKSKILLINFO.TXT`, `SETITEM.TXT`, `ITEMS.TXT`, `FX.TXT`, `VIP.TXT`) with throwaway readers (not in the repo); the perk names are the US string list's. The Steam install was not inspected; its data may differ (Open questions).
Status: draft

Sections: Summary; Units and conventions; Constants; Behaviour (1 what a player wears and where the data lives, 2 body types, 3 vests, 4 gear pieces, 5 helmets, 6 goggles and masks, 7 backpacks, 8 gear mods, 9 the gear list and the armour step, 10 speed, sprint and stamina from gear, 11 ammunition from gear, 12 set items, 13 the perk system, 14 each perk, 15 infection-mode perks, 16 function items and fast respawn, 17 how gear reaches the match, 18 players with no gear: offline, bots, the VIP); Per-tick order; Edge cases; Quirks; Corrections (2026-10-22); Test cases; Open questions.

---

## Summary

- A player's gear is a handful of records picked by the loadout: the **vest** is the character model record itself (`Model#`: armour pool, protection piece, speed penalty, sprint and stamina bonus), the **helmet** is the head piece (`Hair#`), **goggles and masks** are the face attachment (`Goggle#`), the **backpack** is a back attachment plus an inventory item that adds weapon slots. Each of the first three can name a **gear piece** (`Gear#` in the weapons file) that blocks a fraction of damage per damage type and body zone.
- All protection draws on **one armour-point pool** (the vest's `MaxArmor`, 100 for every stock body). Gear only blocks while that pool is above zero; the blocked share of consumable protection is taken from the pool. Helmets and masks have no armour of their own: a headshot is protected only while the vest still has armour points. Armour is subtracted before the zone factor.
- Speed is one combined penalty (weapon + vest + helmet + goggles + backpack + gear mods + figure terms) that scales every speed limit; sprint multiplies run speed by the figure's `DashSpeed` times one plus the sum of the gear sprint bonuses (the vests' 0.4); stamina capacity is the figure's 100 times one plus the gear stamina bonuses. `movement.md` owns that arithmetic; this spec fixes every input and its stock value.
- **Perks** are inventory items that occupy 1–3 of 3 (4 with an extender) perk slots and switch on one of 18 server-managed abilities (drops on kill, footstep volume, sprint on multi-kill, scope steadiness, grenade force, dropped grenade, GP for weapons, body looting, faster respawn, ammo box, team stamina and accuracy buffs, trap indicator, heal). Most have a basic and an "Expert" item with different numbers.
- **Set items** give tiered bonuses (damage, protection, speed, stamina, extra magazines, reload/draw/fire rate, spread, EXP/GP) for wearing several pieces of one set. **Function items** give fast respawns, co-op respawn tokens, EXP/GP boosts and extra slots.
- Gear is fixed when the player joins: the lobby service hands the server the equipped items, the client sends its selection record; nothing expires or changes during a match. Without a lobby (offline, LAN) there are no perks, function items or set items; bots wear only their template's model, head and goggle gear.

## Units and conventions

- Distances in world units (u), up is +Y (`movement.md`). HUD metres are u / 40.
- Speeds in u/s, times in seconds unless "ms" is said. Stamina in stamina points (sp).
- Fractions are plain numbers (0.15 = 15 %). "Penalty" P: speed × (1 − P); negative P is faster.
- Damage types by the game's numbers: 2 bullet, 3 burn, 5 crush, 7 explode, 9 poison (gas), 13 melee, 34 flash, 38 claymore, 39 stamina (kit), 40 infected explode, 41 infected poison. Body zones (locations): 1 head, 2 torso, 3 arm, 4 leg, 5 groin (`weapon_firing.md`, Hit zones).
- Perk timers run on the server's millisecond wall clock (OS tick), checked once per server frame (20 Hz, `game_modes.md`); client perk effects (stamina drain, throw bar) run per rendered frame.
- "Basic / Expert": a perk record's values with suffix `_0` belong to its first item number (basic), `_1` to its second (the "Expert" item). Unsuffixed values apply to both.

## Constants

| Name (ours) | Value | Unit | Meaning |
|---|---|---|---|
| perk slots | 3 (4 with a skill-slot extender item) | slots | A perk takes `SlotNum` adjacent slots |
| backpack weapon slots max | 4 | slots | Extra loadout slots from the backpack item and character items |
| gear-mod slots | 3 each for head, face and vest | slots | `attachments.md`, gear mods |
| multi-kill window | 5000 | ms | A kill within 5 s of the killer's previous enemy kill raises the multi-kill counter; otherwise it restarts at 1 |
| set-item tiers | `ItemCount#` thresholds | pieces | Highest threshold ≤ pieces worn applies |
| fast-respawn key | F7 | key | Spends one fast respawn (16) |
| use-perk key | profile command "Use_Perk", default key code 41 (the key left of 1 on a US keyboard), held, plus 1–4 | keys | Starts the active perk in HUD position 1–4 |
| Fighting Chance rates | ammo 1.25, damage 1.03, speed 1.03, stamina 1.03 | × | Quarantine humans with an infected-mercenary item (15) |
| Warlord box charges | 10 (`RechargeCount`) | uses | Ammo-box refills before it vanishes |

Every other number is data, listed in the section that uses it.

## Behaviour

### 1. What a player wears, and where the data lives

| Slot (role) | Record that gives it gameplay | File in the attribute archive | Gameplay fields |
|---|---|---|---|
| Body type (figure type) | `FigureType#` | `MODELBUTES.TXT` | `HP`, `DashSpeed`, `Maxstamina`, `DashGauge`, `GaugeRec`, `VariableSpeed`, `VariableWeight`, `VariableJump`, `StiffDuration`, `Sex` |
| Vest (character model; mercenaries are full-body vests) | `Model#` | `MODELBUTES.TXT` | `GearId`, `MaxArmor`, `MaxHitPoints`, `MovePenalty`, `DashSpeed`, `MaxStamina`, `Mass`, `ReloadAniRate`, `SelectAniRate`, `FigureType` |
| Helmet, hat, hair (head piece) | `Hair#` | `MODELBUTES.TXT` | `GearId`, `MovePenalty`, `MaxStamina`, `DashSpeed` |
| Face piece | `Face#` | `MODELBUTES.TXT` | `GearId` (no stock face has one) |
| Goggles, masks, beards (face attachment) | `Goggle#` | `ATTACHMENTS.TXT` | `GearId`, `MovePenalty`, `Maxstamina`, `DashSpeed` |
| Backpack (back attachment) | `Backpack#` | `ATTACHMENTS.TXT` | `MovePenalty`, `MaxStamina`, `DashSpeed` |
| Protection piece | `Gear#` | `WEAPONS.TXT` | `Armor`, `Health`, `ProtectionType#`, `NodeHeadProtection#`, `NodeTorsoProtection#`, `NodeArmProtection#`, `NodelegProtection#`, `Consumable#`, `Exclusive` |
| Gear mods (head, face, vest) | `Mod#` with `Kinds` 5, 6, 7 | `MOD.TXT` | `GearID`, `MovePenalty`, `Maxstamina`, `DashSpeed`, `InfectedIncHPPer` |
| Perk ability | `PerkSkill_#` | `PERKSKILLINFO.TXT` | per perk, section 14 |
| Shop items (perks, backpacks, characters, function items) | `Perk#`, `BackpackItem#`, `Character#`, `UseItem#`, ... | `ITEMS.TXT` | `SkillType`, `SlotNum` (perks), `Slot` / `AddSlot` / `SlotType` (backpack slots), `SubType` (function items) |
| Set bonuses | `Effect#`, `SetItem#` | `SETITEM.TXT` | section 12 |
| Warlord box | the `ProjectileFX` record of weapon 499's ammo (`Ammo_Supply`) | `FX.TXT` | `RechargeCount`, `RechargeTime0..7`, `RechargeDefaultTime` |

Keys are case-insensitive (`attribute_data.md`), so `GearId`/`GearID` and `Maxstamina`/`MaxStamina` are the same key. Missing number fields read as 0 unless a section says otherwise; a missing `GearId` is −1 (no piece).

**The shop table's numbers are display only.** `ITEMS.TXT` fields such as `Speed`, `Stamina`, `Armor`, `ArmorGrade`, `Protect_all`, `Protect_head`, `Protect_flash`, `Protect_gas`, `Protect_fire`, `MoveHandicap` feed the item detail panel and nothing else. The gameplay numbers are the records above.

### 2. Body types (figure types)

The figure type is chosen by the vest record's `FigureType` after the face redirect (`characters.md` section 1). The two stock records have identical numbers:

| Field | Type 0 "Male" | Type 1 "Female" | Used for |
|---|---|---|---|
| `HP` | 100 | 100 | Max hit points when the vest has no `MaxHitPoints` > 0.1 |
| `DashSpeed` | 1.0 | 1.0 | Sprint multiplier `D_fig` (10) |
| `Maxstamina` | 100 | 100 | Base stamina capacity (10) |
| `DashGauge` | 30 | 30 | Stamina drained per second while sprinting |
| `GaugeRec` | 25 | 25 | Stamina recovered per second |
| `VariableSpeed`, `VariableWeight` | 0 | 0 | Subtracted / added into the penalty (10) |
| `VariableJump`, `VariableDamage`, `StiffDuration`, the perturb fields | 0 | 0 | `movement.md`, `weapon_firing.md` |
| `Sex` | Male | Female | Sounds; the Quarantine and VIP checks; perk voice lines |

`Speed` 90, `Jump` 57, `Shooting` 70, `WeaponFenalty` 10, `Damagedown` 0 are shown stats; no movement or damage rule reads them (`attribute_data.md`). So a female body moves, sprints and tires exactly like a male one; only the hit skeleton differs (`weapon_firing.md`).

### 3. Vests (character models)

A vest record gives four things: the armour pool (`MaxArmor`), one protection piece (`GearId`), a speed penalty (`MovePenalty`) and sprint and stamina bonuses (`DashSpeed`, `MaxStamina`). It also sets max hit points (`MaxHitPoints` > 0.1, else the figure's `HP`) and the physics mass (`Mass`).

Stock vests (US data; every one has `MaxArmor` 100, `DashSpeed` 0.4, `MaxStamina` 0, `Mass` 2000, no `MaxHitPoints`):

| Model ids (male / female) | Name | `GearId` → piece | Bullet protection torso/arm/leg | `MovePenalty` |
|---|---|---|---|---|
| 0 / 7 | Normal | 0 `AP_Type_Default` | 0.15 | 0.10 |
| 1 / 8 | Light | 1 `AP_Type_Light` | 0.05 | 0.00 |
| 2 / 9 | Heavy | 2 `AP_Type_Heavy` | 0.30 | 0.16 |
| 25 / 26 | balance | 19 `AP_Type_Balanced_Vest` | 0.25 | 0.13 |
| 28 / 29 | balance_NX | 19 | 0.25 | 0.07 |
| 30 / 31 | light_NX | 1 | 0.05 | −0.05 |
| 33 / 34 | Tanker_Vest | 34 `AP_Type_Tanker` | 0.32 | 0.145 |
| 35 / 36 | AdvMedVest | 0 | 0.15 | 0.06 / 0.08 |

Every stock body piece also blocks 0.5 of burn damage on all four zones, head included. Mercenary bodies carry their own numbers, for example `Specialist01_M` (22): piece 52 `Codename_Scorpion` (bullet 0.30, burn 1.0, poison 0.5), `MovePenalty` 0.03, `MaxStamina` 0.2; `Hauser` (24): piece 25 `Commando Vest` (0.35), `MovePenalty` 0.19. Across the 431 model records `MovePenalty` runs from −0.2 to 0.19, `DashSpeed` from 0.09 to 0.6 (103 records have none, so 0) and `MaxStamina` from 0 to 0.5.

**Which record.** The vest's gameplay comes from the selection's model id (the real vest), never from a cosmetic character or cosmetic vest, which change only the look (`characters.md`). Exceptions: the physics mass is read from the cosmetic-resolved record (2000 everywhere, so it never matters), and an infection override record (`Overidable_Model#`) replaces gear, armour, hit points, penalty and stamina while it applies (`characters.md` section 2).

**Armour points.** At every spawn and respawn the pool is set to the vest's `MaxArmor` (the model record's separate `Armor` field is overwritten in the same step and has no lasting effect). Armour only goes down (9); in a match it comes back only through a respawn, an armour pickup, or a gear piece with `Armor` > 0 being added while alive (4).

**Mass** is the engine physics mass of the player object (it sets how hard the player is to push in object collisions). All stock bodies are 2000, so it never differs between players.

### 4. Gear pieces

A `Gear#` record (id = its number; 258 records in the US data) holds, per entry i: a damage type `ProtectionType{i}`, four fractions `NodeHeadProtection{i}`, `NodeTorsoProtection{i}`, `NodeArmProtection{i}`, `NodelegProtection{i}` (each clamped to [0, 1] at load), and `Consumable{i}` (1 = the blocked amount wears the armour pool; missing = 1). For a damage type, a piece uses only its **first** entry of that type. Plus:

- `Armor` (default 0): when the piece is added to a living player, the armour pool is repaired by this much, capped at the max (no change if already full). Every body piece and the anti-flash pieces have 100; head pieces 0.
- `Health` (default 0): healed instead when `Armor` is 0. No stock piece has it.
- `Exclusive`: read, but nothing in the match uses it.

Stock pieces worn by stock items (H/T/A/L = head/torso/arm/leg; "nc" = not consumable):

| Id | Name | Bullet (2) H/T/A/L | Burn (3) | Poison (9) | Flash (34) | Worn by |
|---|---|---|---|---|---|---|
| 0 | `AP_Type_Default` | 0 / 0.15 / 0.15 / 0.15 | 0.5 all | | | Normal vests |
| 1 | `AP_Type_Light` | 0 / 0.05 / 0.05 / 0.05 | 0.5 all | | | Light vests |
| 2 | `AP_Type_Heavy` | 0 / 0.30 / 0.30 / 0.30 | 0.5 all | | | Heavy vests |
| 19 | `AP_Type_Balanced_Vest` | 0 / 0.25 / 0.25 / 0.25 | 0.5 all | | | balance vests |
| 34 | `AP_Type_Tanker` | 0 / 0.32 / 0.32 / 0.32 | 0.5 all | | | Tanker vests |
| 3 | `AP_Head_default` | 0.25 / 0 / 0 / 0 | | | | Combat, goggle and protect helmets |
| 4 | `AP_Head_Midium` | 0.30 / 0 / 0 / 0 | | | | EOD helmet |
| 8 | `AP_Head_Heavy` | 0.325 / 0 / 0 / 0 | | | | Pilot and skull helmets |
| 17 | `Bonehead_Helmet` | 0.35 / 0 / 0 / 0 | | | | IBH Mk2, Bonehead, VBSS helmets |
| 5 | `MK1_Gas_Mask` | | | 0.28 all, nc | | Mk1 gas mask |
| 7 | `MK3_Gas_Mask` | | | 0.35 all, nc | | Mk3 gas mask |
| 18 | `Russian_Gasmask` | | | 0.32 all, nc | | Russian gas mask |
| 28 | `Chemical_Gassmask` | | | 0.50 all, nc | | Chemical gas mask |
| 9, 10 | `AF-II Fire Mask`, `FG-1 Fire Mask` | | 0.35 all | | | the two fire masks |
| 36 | `Anti-Flash` | | | | 1.0 all, nc | Anti-Flash goggles (I and II) |
| 42 | `Anti-Flash_70` | | | | 0.7 all, nc | Metal anti-flash |
| 40 | `Anti-Flash_50` | | | | 0.5 all, nc | Orange anti-flash goggle |
| 38 | `Anti-Flash_30` | | | | 0.3 all, nc | Board goggle |
| 33 | `Santa_Beard` | 0.03 / 0 / 0 / 0 | | | | most skull masks and face guards |
| 23, 24 | `Infected`, `Host_Infected` (mode) | 0.96 all, nc | 0.75 nc | 1.0 nc | 1.0 nc | infection forms; also explode 0.10, melee 0.70 |

Pieces 67–120 (`Protect_all_*`, `Protect_head_*`, `Protect_fire_*`, `Protect_gas_*`, `Protect_flash_*`, `Protect_Explosive_*`) are the protection steps given by gear mods (8); later records (event characters and helmets) mix all four types. No stock piece lists melee (13) for a player-wearable item, and only mode and gear-mod pieces list explode (7) (`weapon_firing.md` Q15, `grenades.md` 9).

There is **no smoke protection**: smoke grenades only block sight (`grenades.md`); "gas" is the poison type.

### 5. Helmets (head pieces)

A head piece gives head protection through its `GearId` piece and may change speed and stamina. Stock examples:

| Hair id | Name | Piece | `MovePenalty` | `MaxStamina` |
|---|---|---|---|---|
| 0 | HBasic (none) | none | 0 | 0 |
| 1, 2, 5 | Goggle_Helmet, Combat_Helmet, protect_helmet | 3 (head 0.25) | 0 | 0 |
| 8 | EOD_helmet | 4 (0.30) | 0 | 0 |
| 11, 12 | Pilot_Helmet, Skull_Helmet | 8 (0.325) | 0 | 0 |
| 16, 19, 22 | IBH_Mk2_Seal, Bonehead_Helmet, VBSS_Helmet | 17 (0.35) | 0 | 0 |
| 9 | Combat_Beanie | none | −0.018 | 0 |
| 10, 15, 17 | Bandana_default, Boonie_Hat, Green_Beret | none | −0.03 | 0 |
| 24 | Black_Beret | none | 0 | 0.2 |

No head piece has `DashSpeed`. Of 355 head pieces, 120 have a gear piece; penalties run from about −0.1 to 0; `MaxStamina` from 0 to 0.45.

**Headshot protection.** On a bullet (or melee) hit whose last-hit node is a head sphere, the head entries of every worn piece are summed (the vest's head entry is 0, so the helmet alone protects), the blocked part comes off before the ×6 head factor, and it wears the shared armour pool (9). With the pool at 0 the helmet does nothing.

### 6. Goggles and masks (face attachment)

The face attachment (`Goggle#`, socket `Eyes`) gives a piece through `GearId` and may change speed and stamina. Of 186 records, 110 name a piece; `MovePenalty` is −0.18 to −0.03 where present (66 records −0.03); `Maxstamina` 0.03 to 0.3 where present. Stock:

| Goggle id | Name | Piece | What it protects | Other |
|---|---|---|---|---|
| 0, 1 | Glass, Inline_Goggle | none | nothing | |
| 2 | Board_Goggle | 38 | flash 0.3 | |
| 39 | Anti-FlashGoggles | 36 | flash 1.0 | `MovePenalty` −0.03 |
| 42 | Anti-FlashGoggles II | 36 | flash 1.0 | `Maxstamina` 0.05 |
| 43 | Orange_AntiFlash_Goggle | 40 | flash 0.5 | |
| 44 | Metal_AntiFlash | 42 | flash 0.7 | |
| 3, 4, 16, 18 | Mk1, Mk3, Russian, Chemical gas masks | 5, 7, 18, 28 | poison 0.28, 0.35, 0.32, 0.50 (not consumable) | |
| 5, 6 | AF-II, FG-1 fire masks | 9, 10 | burn 0.35 | |
| 7–12, 14 | face guards, skulls, Santa Beard | 33 | head bullet 0.03 | |
| 13, 19 | Red Nose, Elite_Red_Nose | none | nothing | `Maxstamina` 0.2, 0.3 |
| 24–38, 45–54 | Worldcup skull masks | 35 `Patriot_Skull_Mask` | head bullet 0.10 | `MovePenalty` −0.03, `Maxstamina` 0.1 |

**Flash** (`grenades.md` section 10): the flash amount is reduced by the sum of the worn pieces' **torso** flash entries plus set-item protection, clamped to [0, 1]; it does not need or use armour points. **Poison and burn** go through the armour step with the torso entries (area damage never uses the zone, `grenades.md` 9): a gas mask's poison protection is not consumable, so it never wears the pool, but like all gear it stops working once the pool is 0.

### 7. Backpacks

A backpack has two parts:

- **The back attachment** (`Backpack#`, socket `1W_N`, chosen by the selection record's backpack id) is cosmetic except for `MovePenalty` (0 to 0.10, a few negative; the stock `normal_backpack_slot0..4` have 0, 0.01, 0.03, 0.03, 0). No backpack record has a gear piece, `MaxStamina` or `DashSpeed`.
- **The backpack item** (`BackpackItem#` in `ITEMS.TXT`) gives **extra loadout weapon slots**: `Slot` (1–4) slots of the normal kind (extra primaries). Character items can add slots too (`AddSlot` 1 or 2 with `SlotType` 0 normal or 1 special), and a "special backpack extender" function item adds its slots when the character's default slot kind is special. At most 4 backpack slots in all. What may go in each slot kind is `inventory.md`'s loadout.

A backpack gives **no extra ammunition and no extra grenades**: each backpack weapon is simply another weapon with its own ammo (`inventory.md` section 1). Extra ammunition only comes from set items and Fighting Chance (11).

### 8. Gear mods

Head, face and vest items can carry up to 3 gear mods each (`attachments.md`, gear mods). Each mod record may give a piece (`GearID`, the `Protect_*` pieces 67–120), a `MovePenalty` (for example −0.005, −0.007, −0.009 for the speed mods), `Maxstamina` and `DashSpeed`; they add into the same sums as the item they sit on (9, 10). `InfectedIncHPPer` raises max hit points by that fraction while the wearer is infected. Gear mods are not applied while infected.

### 9. The gear list and the armour step

**Building the list (server).** At every spawn, respawn and model change, the player's piece list is cleared and refilled, in this order: vest piece, head piece, face piece, goggle piece, then the pieces of the 3 head mods, 3 face mods and 3 vest mods. A missing or invalid id adds nothing. Each added piece with `Armor` > 0 repairs the pool by `Armor` (capped); this happens after the respawn reset, so at spawn it changes nothing. The same piece can appear twice and then counts twice. The client is told the vest, head, face and goggle piece ids.

**The armour step** for one damage event is `weapon_firing.md`'s Armour section; restated with every input:

1. Crush damage (falls) skips gear entirely.
2. The zone ℓ is the last-hit node's location; unknown counts as torso; for damage types that cannot headshot (area, over-time and thrown-weapon damage) the gear's **torso** entries are used.
3. Protection sum: first the **set-item protection** for ℓ (12), counted as consumable; then each piece's first entry for the damage type at ℓ (head, torso, arm or leg; the groin gets 0 from pieces).

```math
P = P_{set}(\ell) + \sum_{pieces} p_i(t,\ell),\qquad c = \frac{P_{set} + \sum_{consumable} p_i}{P}\ (c = 0\ \text{if}\ P = 0),\qquad B = D\cdot\operatorname{clamp}(P,0,1)
```

4. With armour points AP and zone factor z (`weapon_firing.md`): AP = 0 → D′ = D·z (no protection at all); AP > 0 and (AP − B > 0 or c ≤ 0.001) → D′ = (D − B)·z; otherwise D′ = (D − AP)·z. Then AP ← max(0, AP − B·c) when AP > 0.

So: protection from every slot adds up before the clamp; armour comes off before the head ×6; non-consumable protection (gas masks, anti-flash) costs nothing; and an empty pool switches every piece off. Attacker-side factors (set damage, Fighting Chance) apply before this step (Per-tick order).

### 10. Speed, sprint and stamina from gear

`movement.md` (speed limit, sprint and stamina) owns the formulas; the inputs are:

```math
P = \operatorname{clamp}\!\big(P_{weapon}(1+c_{mod}) + P_{vest} + P_{head} + P_{goggle} + P_{pack} + \textstyle\sum P_{gearmod} + w_{fig} - s_{fig} + P_{custom} + P_{stun},\ -1,\ 1\big)
```

```math
V_{sprint,0} = K\cdot D_{fig}\cdot\big(1 + D_{vest} + D_{head} + D_{goggle} + D_{pack} + \textstyle\sum D_{gearmod}\big)
```

```math
V = V_1\cdot(1 - P + S_{set})\cdot m_{mutation}\cdot f_{FC}\cdot f_{slow}\cdot f_{container}
```

```math
S_{max} = \mathrm{Maxstamina}_{fig}\cdot\big(1 + M_{head} + M_{goggle} + M_{vest} + M_{pack} + M_{set} + \textstyle\sum M_{gearmod}\big)\cdot m_{mutation}\cdot f_{FC}
```

- P terms are the `MovePenalty` fields of section 1 (head, goggle and backpack only when worn); `w_fig`, `s_fig` the figure's `VariableWeight`, `VariableSpeed` (0); `P_weapon` the held weapon's `MovePenalty` (zoomed or not, `movement.md`). The penalty is the client's; the server takes the client's movement.
- `S_set` is the set-item speed bonus (12); `m_mutation` = 1 + (buff % + nerf %)/100 from mutation boxes (`game_modes.md` S12; 1 without); `f_FC` = 1.03 for Fighting Chance (15), else 1.
- The client stores S_max as an integer (truncated); the server keeps it as a real number and ends sprint by its own copy (`movement.md`).
- **The VIP of VIP Escort** (not an account type) is special: P = `P_vest` only (weapon and every other term ignored), sprint = K·(1 + `D_vest`) (no figure multiplier, no other bonus), S_max = the figure's `Maxstamina` alone, and the mutation buff and nerf are cleared. All of that player's perks are disabled while they are the VIP (13). The VIP role's own health and armour (`VIP_HP` 300, `VIP_AP` 100 in `VIP.TXT`) belong to `game_modes.md`.

Stock results with an M4A1 (`MovePenalty` 0.10), base run K = 285, walk 70, crouch 50 (V before integration overshoot, `movement.md`):

| Vest | P | Run | Sprint | Walk | Crouch |
|---|---|---|---|---|---|
| Normal | 0.20 | 228 | 319.2 | 56 | 40 |
| Light | 0.10 | 256.5 | 359.1 | 63 | 45 |
| Heavy | 0.26 | 210.9 | 295.26 | 51.8 | 37 |
| balance | 0.23 | 219.45 | 307.23 | 53.9 | 38.5 |
| light_NX | 0.05 | 270.75 | 379.05 | 66.5 | 47.5 |
| Tanker | 0.245 | 215.175 | 301.245 | 52.85 | 37.75 |

Every stock vest gives 100 sp (no `MaxStamina`), drained at 30 sp/s and recovered at 25 sp/s.

### 11. Ammunition from gear

The **maximum carried ammunition** of a weapon (its ammo's `MaxAmount`, `inventory.md`) is raised, in this order:

1. Fighting Chance (15): n ← n + max(1, ⌊1.25·n⌋ − n).
2. Set items (12): n ← n + k·`ShotsPerClip`, where k is the sum of the worn set's "extra magazines" values (truncated to an integer), from the player-wide list and then from the list for that weapon.

At spawn every loadout weapon's total is raised to this maximum (so a set bonus also raises the starting ammunition), and refills and pickups cap at it. A magazine attachment's `MaxAmountPlus` is added on the client before both steps (`attachments.md`). Clip size never changes through gear.

### 12. Set items

`SETITEM.TXT` has effect kinds `Effect0..16` (`EffectType`, optional `FunctionInfo#`) and sets `SetItem0..60`. A set lists its item numbers (`ItemNo#`) and tiers `ItemCount{j}` (a piece count) with effects `ItemCount{j}_Effect{k}_ID` (an effect kind), `_Value`, and an optional condition list `_ItemNo#`.

- **Tier:** count how many *different* listed items are among the player's equipped items (all loadout slots: weapons, vest, head, face, backpack...). The applied tier is the last one whose threshold is ≤ that count; below the first threshold, nothing.
- **Condition list:** an effect without one applies to the player. With one, it applies player-wide if one of the listed items is worn gear, and in any case as a per-weapon bonus for each listed item that is not that first worn match (so a weapon of the set gets its own bonus while held). Values of the same kind add up.
- **Effect kinds** (by `EffectType`; ids 14 and 15 are protection restricted to location 1 head and 2 torso; id 16 is type 14):

| `EffectType` | Effect | Where it applies |
|---|---|---|
| 0, 1, 2, 3 | Damage dealt +v (all victims, players, AI (optionally named templates), world models) | Attacker side, before armour: D ← D·(1 + Σv), player-wide then per weapon |
| 4 | Protection +v (only at the listed locations, else everywhere) | Added to P in the armour step and in flash protection; consumable |
| 5 | Protection +v against AI attackers | Same |
| 6 | Speed +v | `S_set` in 10 |
| 7, 8 | EXP, GP +v | Match rewards |
| 9 | Extra magazines | 11 |
| 10, 11, 12 | Fire, reload, draw animation rate +v | `attachments.md` animation rates (`f_set`, `r_set`, `s_set`) |
| 13 | Stamina +v | `M_set` in 10 |
| 14 | Max spread × (1 − Σv) | The weapon's `MaxPerturb` (`hud_crosshair.md`) |

Example: `SetItem5` (four items) gives at 3 pieces EXP +0.25 and GP +0.25, at 4 pieces +0.4 each. Most sets give a torso or head protection step, speed +0.03 to +0.12, stamina +0.2 to +0.8, extra magazines, and reload and draw rates for their weapons.

### 13. The perk system

- **Owning and equipping.** A perk is a `Perk#` item (`ITEMS.TXT`, item numbers 16001–16024): `SkillType` names the ability, `SlotNum` the slots it occupies (1, 2 or 3) out of the player's 3 perk slots (4 with a skill-slot extender function item). The lobby refuses an arrangement that does not fit. Owning the same ability twice has no extra effect (one entry per ability).
- **Levels.** In `PERKSKILLINFO.TXT`, `PerkSkill_N` with `Type` N; `ItemNo_0` and `ItemNo_1` are the basic and Expert item numbers; suffixed fields pick the level by the item the player owns (Open question 1).
- **Arrival.** When a player joins, the server builds their perk list from the lobby's in-game item list (section 17), drops any ability that is disabled in this mode (`DisableGameType#`, plus the per-perk rules below), and sends the list to the player and, for Specter, Inner Focus, War Cry and Eagle Eye, to everyone. The room option "Perks" (`EnablePerkSkillSetting`, `game_modes.md` S2) switches the whole system off. Observers have none. A perk only works on a human body (not while infected, not on a non-human model).
- **Passive** perks (Resourceful, Specter, Victory Lap, Steady Hand, Pitcher, Parting Gift, Relentless, Caution) act by themselves. **Active** perks (Awareness, Spoils of War, Opportunist, Warlord, War Cry, Eagle Eye, Inner Focus) start with the use-perk key held plus 1–4 (the perk in that HUD position). The client checks local conditions, then either sends "start" or, for perks with a `WaitTime`, enters a **wait** and asks the server to start after the wait. During a wait, pressing any movement key (forward, back, strafe, jump) or dying cancels it.
- **States:** normal (ready), begun / in use, waiting, failed, disabled (cooling down or used up). After a successful start of a perk with `CoolTime`, it is disabled for that many seconds, then ready again. A perk's effect time (`Time`) is counted on the server from the start; at its end the state returns to normal.
- **Buffs** (War Cry, Eagle Eye): at the start, the user and every teammate whose position is strictly closer than `TargetDistance` to the user get the buff for `Time` seconds; a new buff of the same kind replaces the old timer; if the user leaves the match, their buffs end at once. Distance is not rechecked later.
- **VIP Escort:** the VIP's perks are all disabled while they are the VIP and come back when they are not. **Spy Hunt:** the "super spy" loses Inner Focus.

### 14. Each perk

| Type | Name (item numbers) | Slots | Kind | Data (basic / Expert) |
|---|---|---|---|---|
| 1 | Resourceful (16001, Expert 16019) | 1 | passive | `DropProbability` 40 / 50 %, `HealthDrop_HP` 40 / 45, `AmmoDrop_Ammo` 50 / 55 %, `BoxHideTime` 10 s, `BoxLifeTime` 20 s |
| 2 | Awareness (16002) | 1 | active | `SPDecrease` 5 sp/s, `FootSoundVolume` 1.0, `SoundRadius` 0.9, four extra step sounds |
| 3 | Specter (16003, Expert 16020) | 1 | passive | `FootSoundVolume` 0.4 / 0.3, `DashSoundVolume` 1.0 / 0.75 |
| 4 | Victory Lap (16004) | 1 | passive | `StartKill` 3, `Time` 8 s |
| 5 | Steady Hand (16005, Expert 16021) | 1 | passive | `SPDecrease` 10 / 8 sp/s |
| 6 | Pitcher (16006) | 1 | passive | `MinForce` 0.8, `MaxForce` 1.35, `BarRightSpeed` 0.8 s, `BarLeftSpeed` 0 |
| 7 | Parting Gift (16007, Expert 16022) | 2 | passive | `WeaponID` 472 / 846, `Probability` 100 %; off in Quarantine (11) |
| 8 | Spoils of War (16008) | 2 | active | `MaxCount` 10, `AddGP` 10, `TargetDistance` 250 u, `WaitTime` 1.8 s; off in Arms Race (22) |
| 9 | Opportunist (16009) | 2 | active | `TargetDistance` 250 u, `WaitTime` 1.5 s, `OwnerTime` 10 s, `DropWeaponMax` 1; off in Arms Race (22) |
| 10 | Relentless (16010) | 3 | passive | `RespawnTime` 5.6 s |
| 11 | Warlord (16011) | 3 | active | `WeaponID` 499, `WaitTime` 2.5 s, `CoolTime` 180 s, `SoundRange` 2200 u |
| 12 | War Cry (16012, Expert 16023) | 2 | active | `Time` 30 s, `CoolTime` 150 s, `TargetDistance` 1300 / 1625 u |
| 13 | Eagle Eye (16013, Expert 16024) | 2 | active | `Time` 30 s, `CoolTime` 150 s, `Purturb` 0.1, `TargetDistance` 1300 / 1625 u |
| 17 | Caution (16017) | 1 | passive | `FXName` `Ind_Mine_Red` |
| 18 | Inner Focus (16018) | 2 | active | `HealHP` 100 |
| 14, 15, 16 | Virulent, Calloused, Fighting Chance | | mode | section 15 |

Types 19–21 exist in the code's list but have no data record, so they never load and do nothing.

**Resourceful.** When the owner kills an enemy (not themselves, not a teammate, not in the warm-up round) with a weapon of gun type rifle, sniper, pistol, SMG, shotgun, knife or thrown (not a special-class weapon), roll an integer 0–99; below `DropProbability` a box may drop at the victim's position: if the killer's HP < `HealthDrop_HP` a medikit; else if the killer's current weapon has ammo% = ⌊100·total / max⌋ ≤ `AmmoDrop_Ammo` an ammo box (a full-ammo box instead if the killer carries a thrown weapon with 0 ammo); else nothing. The box lives `BoxLifeTime` s, is visible to the killer's team only, can be picked up by the killer alone for the first `BoxHideTime` s, then by anyone who can see it. What each box refills is the pickup item's (Open question 6).

**Awareness.** Can only start with stamina > 0. While on, every *enemy* footstep the owner hears plays at volume × `FootSoundVolume` and radius × `SoundRadius`, plus one random extra step sound; stamina drains `SPDecrease` sp/s (client gauge) and does not recover. It ends on any movement key, on death, and (as a failure) when stamina reaches 0. No cooldown.

**Specter.** Always on. The owner's own footsteps, as heard by every client, play at volume × `FootSoundVolume`; while the owner sprints, × `DashSoundVolume` (so basic Specter does nothing while sprinting, Expert still quietens to 0.75).

**Victory Lap.** When the owner's multi-kill counter (consecutive enemy kills each within 5 s of the previous) reaches `StartKill` or more, the perk is in use for `Time` s from that kill; each further qualifying kill restarts the 8 s. While in use and the run flag is set (not walking, not crouched), the client uses the full sprint speed limit (10) without the server's sprint confirmation; the sprint flag stays off, so the sprint key does nothing and no stamina is spent.

**Steady Hand.** With a sniper rifle (gun type 6) zoomed in and stamina > 0, scope sway stops (`weapon_classes.md` 7.9) and stamina drains `SPDecrease` sp/s on the client gauge without recovering; at 0 stamina the effect stops until the gauge is above 0 again. Out of zoom or with another weapon it is idle (shown disabled for non-snipers).

**Pitcher.** With a thrown weapon selected, holding fire shows a bar g from 0 that rises 1 unit per (`BarRightSpeed`·10) ms = 8 ms, reaching 100 after 800 ms and staying there (`BarLeftSpeed` 0 means it never runs back). The bar does not move while sprinting. On release the client sends the throw force; the server uses it only if the thrower owns Pitcher, else 1 (`grenades.md` section 3, `f_perk`):

```math
f = \mathrm{MinForce} + (\mathrm{MaxForce} - \mathrm{MinForce})\cdot\frac{g}{100} = 0.8 + 0.55\,\frac{g}{100}
```

**Parting Gift.** On the owner's death (any killer, `Probability` 100 %), a live grenade of weapon `WeaponID` with full fuse appears at the body and moves straight up at that grenade's `DropVelocity` (`grenades.md` 14), credited to the dead player, whatever their own ammo. The perk grenades are weaker than the M67: `M67_perk` (472) `AreaDamage` 20, `M67_perk_2` (846) 24. Disabled in Quarantine, and unless both the room's primary and secondary weapon restrictions are "all", "no sniper", "no backpack", "no shotgun" or "no specialist".

**Spoils of War.** Aim at nothing in particular: the client picks the nearest visible player-dropped weapon item within `TargetDistance` that the owner may pick up; after the `WaitTime` (movement cancels) the server re-checks owner rights and the 3D distance (≤ 250 u), removes the item and credits `AddGP` GP. After `MaxCount` successes in a match the perk is disabled for the rest of it.

**Opportunist.** The target is the body under the crosshair within `TargetDistance`; after the wait the server spawns up to `DropWeaponMax` of the primaries that body carried in its backpack slots (`inventory.md` 12) at its torso ±20 u in x and z with a random yaw, reserved for the owner for `OwnerTime` s (then their team may take them). A body with none fails with "empty backpack"; looted bodies are emptied.

**Relentless.** Only in Elimination, VIP Elimination, clan Elimination, One Man Army, Arms Race, CTF, clan CTF, Spy Hunt, Bombing Run, Seize and Secure, Hired Guns (both) and bot Elimination. The client's respawn wait becomes min(wait, `RespawnTime`) = 5.6 s instead of 7.0 s; the server's lower limit is min(5.0, `RespawnTime`) = 5.0 s, unchanged (`game_modes.md` S5).

**Warlord.** After the 2.5 s wait (movement cancels), the server fires weapon 499 (`PerkAmmoSupply`) from the owner's position along their facing; the thrown box lands (impact sound heard within `SoundRange`) and becomes a supply point with `RechargeCount` 10 charges, owned by the thrower. A living, non-infected player touching it starts a charge for the gun type they hold, if that type has a charge time (`RechargeTime{gun type}`: knife 1, pistol 2, shotgun 2, SMG 2, rifle 3, machine gun 2, sniper 4, thrown 2 s; other types cannot charge), and one charge is reserved. If they stay exactly in place for the charge time, the held weapon (if still that gun type and not marked ignore-recharge) is refilled to its ammo's `MaxAmount`. Moving, dying or becoming infected cancels and returns the charge. The box is removed when charges run out (after the last charge in progress), when its owner leaves, when the owner places a new box, or when the round ends; it can be shot down. Disabled in knife-only rooms. Cooldown 180 s.

**War Cry.** Buff (13): for 30 s the user and teammates within the distance lose no stamina: no sprint drain on the server and on the client, and no perk drain (Awareness, Steady Hand) either. Cooldown 150 s.

**Eagle Eye.** Buff (13): for 30 s the receivers' weapon spread limits shrink: min ← min − ⌊min·0.1⌋, max ← max − ⌊max·0.1⌋ (integer spread units), and the crosshair radius r ← 0.9·r (`hud_crosshair.md`). Weapons without spread are unaffected. Cooldown 150 s.

**Caution.** Always on (not while infected): enemy planted traps (land traps and the other placed weapons) show the red indicator effect.

**Inner Focus.** Can start only below full HP. The server heals `HealHP` (capped at max), sets stamina to 0 and stops stamina recovery; the perk is then used up for this life. It becomes ready again (and recovery resumes) on death or at the start of the next round. In Quarantine it is disabled for players not on the human team.

### 15. Infection-mode perks

Quarantine (`game_modes.md` M8) uses three hard-wired abilities granted by "infected mercenary" items, not equippable:

- **Virulent** (14) on an infected form: sprinting costs no stamina.
- **Calloused** (15) on an infected form: the standing-still heal per tick is × `HPRecoverRate` 2.0.
- **Fighting Chance** (16) for a *human* who owns an infected-mercenary item: max and spawn ammunition n ← n + max(1, ⌊1.25n⌋ − n); damage dealt D ← D + max(1, 1.03·D − D) for positive D; speed limit × 1.03; stamina capacity × 1.03 (`MaxAmmoRate`, `DamageRate`, `SpeedRate`, `StaminaRate`).

### 16. Function items and fast respawn

Function items (`UseItem#`, by `SubType`) reach the server with the lobby's in-game item list (17):

- **Fast respawn.** In Elimination, One Man Army, CTF, Spy Hunt, Bombing Run, Seize and Secure, Hired Guns, Arms Race and VIP Elimination, a dead player can press F7 while waiting; the client sends its respawn request at once with a "fast" flag. The allowance is an account counter from the lobby (remaining / maximum fast respawns, shown on the death screen), unlimited with a monthly-pass item. The server accepts the fast flag only on the Elimination and CTF request messages: it refuses (logged as illegal, player stays dead) if the counter is 0 and there is no pass, otherwise it spends one and respawns without the time check. This is `game_modes.md`'s "fast respawn item".
- **Respawn tokens** and **party respawn tokens**: co-op modes only (`game_modes.md` M19); the count is sent to the client at join.
- **EXP / GP boosters** add their bonus rates to match rewards (summed). Not gameplay.
- **Skill-slot extender**: 4 perk slots instead of 3. **Special-backpack extender**: section 7.

### 17. How gear reaches the match

1. **Loadout (lobby).** The player equips items in the lobby (`ui_system.md`, loadout); rental expiry is the lobby's business and happens between matches.
2. **Join.** The lobby service gives the game server the player's equipped item numbers (all loadout slots), the in-game item list (perks and function items with remaining counts) and account flags. The client sends its character info: the selection record of `characters.md` section 1 (model, head piece, face, backpack, goggle, cosmetics, colours) and its equipped item numbers and gear-mod ids.
3. **Validation (server).** The selection record is checked as in `characters.md` (unknown model: rejected; unknown face, head, backpack or goggle: none). The client's equipped item numbers must equal the lobby's, or the player is kicked ("invalid user info"); an ESL-items-only room skips in-game items that are not on its allow list. The server does not cross-check that the selection's model, head and goggle are items the player owns.
4. **Setup.** Perks are installed (13), set tiers computed from the equipped items (12), the loadout weapons created with ammo at its maximum (11), health and armour reset from the vest (3), stamina capacity computed (10), and the piece list built (9) at every spawn.
5. **During the match nothing changes**: an item that expires mid-match keeps working until the player leaves; there is no in-match equip. Gear-pickup map objects exist in the code but are legacy (Open question 8).

### 18. Players with no gear: offline, bots, the VIP

- **No lobby (offline, LAN, single-player builds):** no perks, no function items, no fast respawns; set items only from whatever equipped list the client sends. A fresh selection record is model 0 (Normal vest: piece 0, `MovePenalty` 0.10, `DashSpeed` 0.4, `MaxArmor` 100), head piece 0 (none), no goggle, no backpack (`characters.md`). That is the reference character of `movement.md` and `weapon_firing.md`.
- **Bots** (`bots.md`): the template's model gives the vest piece and penalty; in bot modes the chosen head piece, face and goggle pieces are added too; bots have no gear mods, perks or set items. Their speed uses the same penalty sum (`bots.md`).
- **VIP of VIP Escort:** section 10.

## Per-tick order

Damage event (server), for one victim:
1. Attacker side: base damage, roll, range band (`weapon_firing.md`), then Fighting Chance, then set-item damage (player-wide, then for the weapon), then attachment bullet bonuses (`attachments.md`).
2. Victim side: crush skips 3–4.
3. Protection sum P and consumable share c: set protection first, then pieces in list order (vest, head, face, goggle, head mods, face mods, vest mods).
4. Armour case and zone factor, armour points reduced; then HP.
5. Flash damage instead: flash protection from the same sum, no armour (`grenades.md` 10).

Server frame:
1. Perk timers for every player: effect time, wait time (start the perk when it ends), cooldown; buff timers; during a round's start phase Inner Focus is made ready.
2. Stamina (server copy): stamina-kit refill, sprint drain unless War Cry or Virulent, recovery unless Inner Focus is in use.
3. Warlord boxes: charge timers, refills, removal.

Client frame:
1. Movement ticks (speed limit with the gear penalty, Victory Lap sprint).
2. Stamina gauge: drain = sprint drain (if sprinting) + Awareness + Steady Hand, or 0 under War Cry / Virulent; recovery otherwise (0 rate after Inner Focus).
3. Pitcher bar.

## Edge cases

- **Two items changing the same number add up**: penalties, sprint bonuses, stamina bonuses and protection fractions are summed over all worn items before any clamp; the penalty is clamped to [−1, 1] after the sum, protection to [0, 1]. The same piece worn twice (for example goggle and mod giving the same piece) counts twice.
- **Empty armour pool**: every piece stops, including gas masks and helmets; flash protection still works.
- **Groin**: pieces give nothing there; set protection without a location list still applies (it is added for any zone).
- **Area damage and set protection**: pieces use torso entries, but set protection restricted to a location uses the last-hit node's location.
- **Max ammo raised by a set while a weapon is held**: the cap rises immediately; the total is topped up only at the next spawn or refill.
- **Respawn during Victory Lap**: dying does not end it in the code read; a respawn inside the 8 s keeps it (Open question 5).
- **Inner Focus at full HP**: refused by the client ("full HP"); dead players and non-humans are refused by the server.
- **Relentless and Fast respawn together**: the fast flag skips the time check entirely.
- **Leaving the match** ends buffs the player gave others and removes their Warlord box.
- **Expert and basic of the same perk owned together**: one entry per ability; which level wins is not defined by the code read (Open question 1).

## Quirks

1. **Ids sent as bytes (do not keep by default).** The server passes the vest, face and goggle piece ids as one byte each (the head piece as two). "None" (−1) therefore arrives as 255, and ids above 255 wrap. In the US data, which has 258 pieces, piece 255 is a real record (`Heartseeker Glasses`: head bullet 0.17, flash 0.30, no armour), so **every player without a face piece (all of them) and without a goggle piece gets it, twice**: +0.34 head bullet protection and +0.60 flash protection. The vest `Heartseeker Uniform` (piece 257) arrives as piece 1 (Light, 0.05 instead of 0.54). With 255 or fewer pieces nothing of this happens. Implement full-width ids and "none = no piece"; offer a faithful mode that wraps ids to a byte and treats 255 as a real piece, for comparing with the reference.
2. **Helmets need vest armour.** Head protection lives on the shared pool; a player whose vest armour is gone takes full headshots. Keep.
3. **Gas masks stop at zero armour** even though they never consume armour. Keep.
4. **Victory Lap ignores the server's sprint confirmation** and costs no stamina. Keep.
5. **Inner Focus leaves the player without stamina recovery for the rest of that life.** Keep.
6. **Basic Specter is silent only while not sprinting.** Keep.
7. **Awareness ends on any movement key**, so it is a stand-still listening tool. Keep.
8. **Shop stats are cosmetic** (section 1): the numbers players see in the item panel can differ from the real ones. Use the records.
9. **Goggle pieces do not match goggle names** in places (the "FP-1 Face Guard" goggle uses piece 33 at 0.03, not piece 11 "FP-1 Face Guard" at 0.05). Data; keep.

## Corrections (2026-10-22)

From a comparison of the reference's server spawn loadout, weapon ammunition
bookkeeping, rechargeable pickup boxes, perk manager and Warlord supply box
with this spec and our implementation. Each entry: what the reference does,
what we do (file:line), and a testable expectation. Numbers are the stock
data of this spec.

1. **Set-item extra magazines do not add starting ammunition; the client
   only shows them** (section 11, Open question 9, third bullet).
   - Reference: a loadout weapon's server total at spawn is the ammo's
     `SpawnedAmount` after Fighting Chance (n + max(1, ⌊1.25n⌋ − n)),
     clamped to the player's maximum. The set-item extra magazines are not
     applied to it; they only raise the maximum (the cap for pickups and
     refills). The message that gives the weapon to the client, however,
     carries the player's **maximum** as the round count, and the client
     uses that as its total. The server ignores any fire message that
     arrives while its own total is 0 (no round, no damage, no
     projectile), so the rounds between the two numbers are shown and can
     be "fired" on the client but do nothing on the server. With stock
     data and no set, only the M-23F and G7A differ (`SpawnedAmount` 1,
     `MaxAmount` 2).
   - Ours: the starting total is raised to the maximum, set magazines and
     Fighting Chance alike (`games/combat_arms/src/game/gear.rs:881-884`),
     so the extra magazines are real rounds.
   - Why it matters: a set with extra magazines gives no extra rounds
     until a refill; our players get up to a magazine more per weapon at
     every spawn.
   - Expect: test case 28 changes: M4A1 with a set of extra magazines
     value 1 → maximum 150, starting total 120 (server); a Warlord or
     full-ammo refill then reaches 150. Fighting Chance (test case 30) is
     unchanged: starting total and maximum 150. Recommendation: keep the
     server's numbers (start at `SpawnedAmount`, raise only the cap) and
     show the real total; the client's inflated display is a fidelity
     option at most.
2. **Resourceful's medikit heals to full and repairs armour to full**
   (section 14, Open question 6).
   - Reference: the medikit box restores hit points to the maximum and the
     armour pool to `MaxArmor`. `HealthDrop_HP` is only the killer's
     health threshold that chooses a medikit, not the amount. A player
     already at full health and full armour cannot take it: the box stays
     for someone else (or until it expires).
   - Ours: heals `HealthDrop_HP` hit points, leaves armour alone, and the
     box is used up by any touch (`games/combat_arms/src/game/perks.rs:1124-1128`,
     `1112-1113`).
   - Expect: killer at 30 HP and 20 armour takes their medikit → 100 HP,
     100 armour. A teammate at 100 HP and 100 armour walks over it after
     the 10 s → nothing happens, the box stays.
3. **Resourceful's ammo box refills the last rifle or SMG held, not the
   weapon in hand, and stays when it cannot refill.**
   - Reference: the ammo box refills one weapon: the one the server
     records as the player's "last primary", which is the last weapon
     switched to whose gun type is SMG, assault rifle, machine gun, sniper
     or rocket launcher (shotguns, pistols, knives and grenades never
     become it). It is refilled only if its gun type is assault rifle,
     else only if it is SMG; machine guns and snipers get nothing. The
     refill sets the total to the player's maximum (the raised one of
     section 11) and happens only when the total is below the ammo's
     record `MaxAmount`. Taking the box needs a refill to happen; otherwise
     it stays.
   - Ours: sets the held weapon's reserve to its ceiling, any weapon with a
     magazine, and the box is always used up (`perks.rs:1129-1135`,
     `1112-1113`).
   - Expect: the killer holds a pistol at 5/45 after emptying a rifle
     (M4A1 at 10/120, last primary) → the box sets the M4A1 to 120 and the
     pistol stays at 5. A killer whose last primary is an M60 or an L96A1
     cannot take an ammo box; it stays and a teammate with a rifle can.
     (The roll that chooses the box still uses the killer's **current**
     weapon, section 14; a thrown weapon in hand counts by its own
     count / maximum, so a killer holding a spent grenade, 0 %, gets an
     ammo box. Ours skips weapons without a magazine there, `perks.rs:944-947`.)
4. **Resourceful's full-ammo box refills every weapon, grenades included.**
   - Reference: every carried weapon except mission items and weapons
     flagged to ignore refills, whose total is below its record
     `MaxAmount`, is set to the player's maximum; thrown weapons, mines and
     air strikes also get a full clip. As in 3, the box stays when nothing
     was refilled.
   - Ours: only weapons with a magazine, reserve to its ceiling; grenades
     stay at 0 (`perks.rs:1136-1143`).
   - Expect: a killer who threw their only M67 and holds an M4A1 at
     30/120 takes the full-ammo box → M67 1 (and selectable again), M4A1
     120.
5. **Warlord's supply point needs the use key, works for the owner's team
   only, and a charge follows the gun type, not the weapon.**
   - Reference: a charge starts when a player inside the box's volume
     holds use (E), on the client's request; the client only offers it
     when the box belongs to the player's team. The server starts it if a
     charge is left, the held weapon's gun type has a `RechargeTime`, the
     weapon is not flagged to ignore refills, and its total is below the
     player's maximum; one charge is reserved. Releasing E, moving (any
     change of position), dying or becoming infected cancels it and gives
     the charge back. When the charge time ends, the weapon then in hand is
     refilled if its gun type is the one recorded at the start (so a switch
     to another rifle still gets the refill), to the player's maximum, and
     only if its total is below the record `MaxAmount`; the charge is used
     either way.
   - Ours: any living, uninfected player within 100 u starts a charge
     without a key, whatever their team; a weapon switch cancels and
     returns it (`perks.rs:1178-1210`, `1158-1166`).
   - Expect: an enemy standing on the box with an empty rifle → nothing.
     A teammate with an M4A1 at 40/120 standing still without E → nothing;
     holding E for 3.0 s → 120, charges 9; releasing E at 2.0 s → no refill,
     charges back to 10. Switching to a backpack rifle at 2.0 s and
     holding still → that rifle is refilled at 3.0 s.
6. **Warlord's box is thrown from the body and goes when the round is
   decided.**
   - Reference: the box is weapon 499's projectile, fired from the
     player's object position (the hull centre, not the eye) along the
     body's facing (yaw only), and it becomes the supply point where it
     lands. It is removed as soon as the round is decided (the end-of-round
     phase starts), not at the next round's start.
   - Ours: the point appears at once 200 u ahead at the thrower's floor
     level (`perks.rs:1062-1067`, a known shortcut) and is removed when the
     round counter changes (`perks.rs:1151`).
   - Expect: a box placed during a round is gone in the 3 s end-of-round
     phase; nobody can charge from it after the round is won. Where it
     lands follows weapon 499's projectile record (not in this spec: Open
     question 10).
7. **Opportunist removes the body it loots and reserves the weapons.**
   - Reference: on success the body is removed from the world. The
     weapons appear at the body's torso node, each offset by a uniform
     random amount in [−20, 20] u on x and z, with a random yaw of 0 to
     1 rad, visible and reserved for the looter alone for `OwnerTime`
     (10 s), then for the looter's team. With no backpack weapons on the
     body the perk fails ("empty backpack") and the body stays. While the
     wait runs, the target body is frozen (its own updates are paused, so
     it does not fade out), and resumes if the perk fails.
   - Ours: the body stays (marked looted); the weapons are dropped at its
     left foot with no offset and no reservation; an empty body still
     counts as a success (`perks.rs:696-711`).
   - Expect: Opportunist on a body with one backpack M4A1 → after 1.5 s the
     body is gone, an M4A1 lies within 20 u of where its torso was; another
     player cannot pick it up for 10 s, a teammate of the looter can after
     that. On a body without backpack weapons → failure, the body stays.
8. **Parting Gift is always the perk's own frag, on a 2.5 s fuse.**
   - Reference: the grenade is weapon `WeaponID` (472 basic, 846 Expert:
     the perk frags of section 14), whatever the dead player carries, made
     at the dead player's object position, moving straight up at that
     weapon's `DropVelocity`. Its fuse is fixed at 2.5 s, not its
     projectile's `Lifetime`. The `Probability` roll applies (100 % in
     stock data).
   - Ours: the template is the first thrown weapon the victim carries, so
     a player carrying an XM84 or M18 drops a flash or smoke with the
     perk's area damage, and a player with no thrown weapon drops nothing
     (`perks.rs:980-1002`).
   - Expect: a Parting Gift owner with an XM84 as their thrown item dies →
     a frag (area damage 20 basic, 24 Expert, radius of the perk weapon's
     ammo) goes off 2.5 s later at the body; no flash. A loadout without a
     thrown item gives the same.
9. **War Cry and Eagle Eye reach dead teammates too.**
   - Reference: the buff goes to the user and every teammate whose
     position is strictly closer than `TargetDistance`, with no check that
     the teammate is alive, so a dead teammate in range is given the
     buff's timer too (whether their respawn clears it was not traced:
     Open question 11).
   - Ours: dead teammates are skipped (`perks.rs:675`).
   - Expect: War Cry used with a dead teammate 500 u away → that teammate
     is on the buff list for 30 s (the HUD's buff icon for them); live
     check of what a respawn inside the 30 s does (Open question 11).
10. **The multi-kill window is strictly under 5 s.**
    - Reference: a kill raises the counter only if it comes less than
      5000 ms after the previous enemy kill; exactly 5000 ms restarts it at
      1. Kills of teammates (and of oneself) neither count nor reset it.
    - Ours: ≤ 5.0 s counts (`perks.rs:893`).
    - Expect: Victory Lap, enemy kills at 0, 5.000 and 6 s → counter 1, 1,
      2: not active (ours: 1, 2, 3, active at 6 s).

## Test cases

Stock US data, M4A1 (35 damage, roll 1.0, `MovePenalty` 0.10, ammo max 120, clip 30) unless stated; victim at full health 100 and armour 100; quirk 1 off unless stated; speeds are the speed limit V.

| # | Setup | Event | Expected |
|---|---|---|---|
| 1 | Normal vest (model 0), no helmet, goggle, backpack | run, sprint, walk, crouch limits | P 0.20; 228, 319.2, 56, 40 u/s; stamina cap 100 |
| 2 | Light vest (1) | run, sprint | P 0.10; 256.5, 359.1 |
| 3 | Heavy vest (2) | run, sprint, walk | P 0.26; 210.9, 295.26, 51.8 |
| 4 | light_NX vest (30), knife held (`MovePenalty` 0) | run, sprint | P −0.05; 299.25, 418.95 |
| 5 | Normal vest + Bandana_default (Hair10) | run | P 0.17; 236.55 |
| 6 | Normal vest + Anti-FlashGoggles (Goggle39) + normal_backpack_slot2 (Backpack2) | run | P 0.10 − 0.03 + 0.03 = 0.20; 228 |
| 7 | Specialist01_M (22) + Black_Beret (Hair24) | stamina cap; sprint 4.000 s from full | cap ⌊100·1.4⌋ = 140; gauge 20 |
| 8 | Normal vest + Red Nose (Goggle13) | stamina cap | 120 |
| 9 | Female Heavy (9) vs male Heavy (2) | run, stamina, HP | identical: 210.9, 100, 100 |
| 10 | Normal vest | torso bullet 35 | B 5.25, HP −29.75, AP 94.75 |
| 11 | Heavy vest | torso bullet 35 | B 10.5, HP −24.5, AP 89.5 |
| 12 | Light vest | torso bullet 35 | B 1.75, HP −33.25, AP 98.25 |
| 13 | Normal vest + Combat_Helmet (Hair2) | head bullet 35 | B 8.75, HP −157.5 (dead), AP 91.25 |
| 14 | Normal vest + EOD_helmet (Hair8) | head bullet 35 | B 10.5, D′ 147, AP 89.5 |
| 15 | Normal vest + Bonehead_Helmet (Hair19), AP 50 | head bullet 20 (pistol-like) | B 7, D′ 78, AP 43 |
| 16 | Combat_Helmet, AP 0 | head bullet 35 | 210, AP 0 |
| 17 | Normal vest + AF-II Fire Mask (Goggle5) | burn 10 | P 0.85, B 8.5, HP −1.5, AP 91.5 |
| 18 | Normal vest + Mk1 gas mask (Goggle3) | poison 10 | B 2.8, HP −7.2, AP stays 100 |
| 19 | As 18 with AP 0 | poison 10 | HP −10 |
| 20 | Heavy vest | M67 at blast centre (explode 60) | HP −60, AP stays 100 |
| 21 | Combat_Helmet + Normal vest | knife head hit 45 (melee) | no piece lists melee: 45 × 6 = 270, AP 100 |
| 22 | Anti-FlashGoggles | flash amount 25 | 0 → not flashed, AP unchanged |
| 23 | Board_Goggle (0.3) / Metal_AntiFlash (0.7) | flash amount 25 | 17.5 / 7.5 |
| 24 | Normal vest + Anti-FlashGoggles, AP 60 at spawn-time gear rebuild (model change while alive) | pieces re-added | AP repaired by 100, capped: 100 |
| 25 | Quirk 1 on, Normal vest, no goggle | head bullet 35 | head P 0.34: B 11.9, D′ 138.6, AP 88.1 |
| 26 | Quirk 1 on, no goggle | flash amount 25 | P 0.60 → 10 |
| 27 | Set with torso protection +0.1 (effect 15) active, Normal vest | torso bullet 35 | P 0.25, B 8.75, HP −26.25, AP 91.25 |
| 28 | Set with extra magazines value 1 for the M4A1 | spawn | M4A1 max and starting total 150 |
| 29 | Set speed +0.06, Normal vest | run | 285 × (1 − 0.2 + 0.06) = 245.1 |
| 30 | Quarantine human with an infected-mercenary item, Normal vest | ammo max; torso bullet 35 dealt to an unarmoured target; run; stamina | 150; 36.05; 234.84; 103 |
| 31 | As 30 | 10-damage pistol hit, no armour | 10 + max(1, 0.3) = 11 |
| 32 | Resourceful basic; roll passes; killer HP 30 | kill an enemy with an M4A1 | medikit at the victim; only the killer can take it for 10 s; gone after 20 s |
| 33 | Resourceful basic; roll passes; killer HP 80, current weapon 50/120 | kill | 41 % ≤ 50: ammo box |
| 34 | Resourceful basic; roll passes; killer HP 80, 100/120 | kill | 83 %: nothing |
| 35 | Victory Lap, Normal vest + M4A1 | enemy kills at 0, 3, 6 s | from 6 s to 14 s running limit 319.2, stamina unchanged, sprint key inert |
| 36 | Victory Lap | kills at 0, 6, 9 s | counter 1, 1, 2: never active |
| 37 | Pitcher, M67 (`Velocity` 1550) | hold fire 0 / 400 / 1000 ms, release | force 0.8 / 1.075 / 1.35; speed 1240 / 1666.25 / 2092.5 u/s |
| 38 | No Pitcher, client reports force 1.35 | throw | server uses 1.0: 1550 u/s |
| 39 | Steady Hand basic / Expert, sniper zoomed, standing, gauge 100 | 2.000 s | gauge 80 / 84 |
| 40 | Awareness on at gauge 100 | stand 4.000 s, then press forward | gauge 80 when the key is pressed; perk off; gauge then recovers 25 sp/s |
| 41 | War Cry basic used | teammates at 1200 u and 1400 u | 1200 buffed, 1400 not; Expert buffs both (1625) |
| 42 | War Cry buff on, gauge 100 | sprint 5.000 s | gauge 100 |
| 43 | Eagle Eye buff, M4A1 (min 3, max 100) | spread limits | min 3, max 90 |
| 44 | Inner Focus, HP 40, gauge 70 | start, then wait 10 s | HP 100, gauge 0 and still 0 after 10 s; after death and respawn the perk is ready and the gauge full |
| 45 | Inner Focus at HP 100 | start | refused, nothing changes |
| 46 | Spoils of War | 11 successful weapon pickups in a match | 100 GP; the 11th does nothing, perk disabled |
| 47 | Relentless, Elimination | die, wait | request at 5.6 s, accepted (server limit 5.0 s) |
| 48 | Elimination, 2 fast respawns left, no pass | die, press F7 at 1 s | respawn at once; counter 1 |
| 49 | Elimination, 0 fast respawns, no pass | die, press F7 | F7 does nothing (client); a forged fast request is refused and the player stays dead until the round setup |
| 50 | Warlord box, player holds an M4A1 with 40/120 | touch, stand still 3.0 s | ammo 120; box charges 9 |
| 51 | As 50 | move at 2.0 s | no refill; box charges back to 10 |
| 52 | Perk slots 3 | equip War Cry (2) + Resourceful (1), then try Relentless (3) | first two fit; Relentless refused; with the extender (4 slots) War Cry + Resourceful still leave 1 slot, Relentless still refused |
| 53 | VIP Escort, the VIP wears Heavy vest + Combat_Helmet + Red Nose, holds an M4A1 | run, sprint, stamina cap | P 0.16; run 239.4; sprint 285 × 1.4 × 0.84 = 335.16; cap 100; perks disabled |
| 54 | Gear item expires during the match | keep playing | still in effect until the player leaves |
| 55 | Offline (no lobby), default selection | join | no perks, no fast respawn; Normal vest numbers (test 1, 10) |
| 56 | Bot with template model Heavy (2) | run speed | p_gear 0.16 in `bots.md`'s formula |

## Open questions

1. **Perk levels.** The data defines basic and Expert values (`_0`, `_1`) per item number, but the code read has the per-level lookup behind a build switch that is off in every project file; with it off, only unsuffixed keys are read and most suffixed perks get degenerate values (Resourceful never drops, Specter mutes footsteps, Steady Hand costs nothing, War Cry and Eagle Eye reach nobody but the user). This spec assumes the shipped clients had levels on. Check in the live game: does Resourceful drop, do Expert items behave differently? Also: if a player owns both levels, which applies?
2. **Steam data.** All numbers here are the US data set's. The Steam install's attribute archive is keyed; re-read the gear, model, attachment, perk and set tables there, and especially the gear-record count (quirk 1 applies only above 255 pieces).
3. **Quirk 1 in the live game.** Confirm a default character with no goggle takes 138.6, not 210, from an M4A1 headshot at full armour in the US build; if live play shows 210, the byte path was different in the shipped build.
4. **Use-perk input.** Default key code 41 plus 1–4 was read from the profile defaults; the level-switch code also adds mouse-wheel selection and wheel click. Confirm the live bindings and whether the number keys still switch weapons while the perk key is held.
5. **Victory Lap and death.** The perk's own text says it resets on death; the code read does not reset it. Test: three quick kills, die, respawn within 8 s.
6. **Resourceful boxes.** What the medikit, ammo and full-ammo boxes give (amounts, which weapons) is the rechargeable pickup item's behaviour, not traced. Same for the respawn-token flow in co-op.
7. **Warlord refill.** The refill adds the ammo up to `MaxAmount`; whether it uses the set-item or Fighting Chance maximum (the code path seen uses the plain record value) and what gun types without a `RechargeTime` entry do (`RechargeDefaultTime` 2.0 is read but its use was not found) need a live check.
8. **Gear pickups.** A map gear-pickup object sends a shorter gear message than the server reads; if a stock map places one, the player's piece list would be replaced. No stock map was checked.
9. **Contradictions with existing specs.**
   - `movement.md` says "A VIP account uses `P = P_vest` only": the flag is the VIP *role* of VIP Escort (the escorted player), not an account type.
   - `game_modes.md` S5 calls fast respawn an "item"; it is an account counter from the lobby (or unlimited with a monthly pass), spent with F7, and only the Elimination and CTF request messages carry it.
   - `inventory.md` section 1 says each weapon starts with its ammo's spawned amount; the server raises every loadout weapon's total to its maximum at spawn, so set-item extra magazines and Fighting Chance raise starting ammo (no difference with stock data and no bonuses).
   - `grenades.md` section 10's flash table is a subset: many later helmets, masks and characters (Anubis, Horus, Bastet, Heartseeker, Agent and event pieces) also carry flash entries, and quirk 1 adds 0.60 to everyone without a goggle piece. Its test cases hold for "no gear" victims with quirk 1 off.
   - `weapon_firing.md` test 8 ("default body gear only, head hit → 210") and `grenades.md`'s no-gear flash cases are what this spec's default (quirk 1 off) gives; the reference with the US data gives the values of tests 25 and 26.
   - `attribute_data.md` says backpack and goggle records carry sprint bonuses: the fields are read, but no stock goggle or backpack record has `DashSpeed`, and backpacks have no stamina either.
10. **Warlord's flight** (Corrections 6). Weapon 499's ammo and projectile
    record (speed, gravity, fuse, model and its touch volume) were not
    read, so where the thrown box lands and how big its use volume is are
    open. Live check: stand on flat ground, use Warlord, measure the box's
    distance from where you stood.
11. **Buffs on dead teammates** (Corrections 9). A dead teammate in range
    is given the buff; whether a respawn before its end keeps it was not
    traced. Live check: War Cry next to a teammate's body, have them
    respawn at once, sprint.
12. **Shown but unreal rounds** (Corrections 1). With a set's extra
    magazines the client shows and fires more rounds than the server
    counts. Live check: wear a set with extra magazines, fire a full
    loadout's worth at a wall, then at a player: do the last magazine's
    hits register?
