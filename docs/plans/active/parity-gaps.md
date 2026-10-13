# Parity gaps with CS:S (what players see and hear)

Started 2026-10-13. An inventory of known and suspected differences
between Lucker Party and Counter-Strike: Source, by area, from (a) refcmp
on the maps with reference captures, (b) every tech-debt row and spec
open question that marks behaviour as unmeasured or ours, (c) a check of
what CS:S players commonly use (the install's `cfg/config_default.cfg`
binds, its sound scripts, its `resource/ui/*.res` panels, common console
commands). Fixes that need no new capture are done from the top; the
rest wait on the captures listed at the end.

Not covered here (other work in flight, 2026-10-13): mg_jacks (water
through geometry, skybox inside players, head-height camera, its say
lines, sounds and strips, the AK's sound fade) and bots (parked). Player
shadows vs decals, the third-person AK magazine and buy zones round
spawns landed on main during this pass.

Status: **have** (matches, measured or from data), **differs** (in, but
ours or unmeasured), **missing**. Impact: how often a player notices
(H every round or fight, M every session, L rarely). Cost: S (hours), M
(a day or two), L (more, or needs a spec session).

## Summary

| Area | have | differs | missing | fixed now |
|---|---|---|---|---|
| Movement | 2 | 7 | 2 | 0 |
| Weapons, recoil, spread | 5 | 8 | 2 | 3 |
| Sounds | 6 | 7 | 5 | 3 |
| HUD | 14 | 4 | 1 | 5 |
| Menus and commands | 12 | 4 | 7 | 9 |
| Rendering, lighting, materials | 4 | 16 | 2 | 0 |
| Maps and entities | 5 | 10 | 1 | 0 |
| Rules and economy | 7 | 4 | 0 | 3 |
| Networking feel | 4 | 2 | 4 | 0 |
| **Total** | **59** | **62** | **24** | **23** |

"Fixed now" counts rows that moved from missing or differs to have in
the first pass, round 2 and the join flow (counted under have; `invnext` and `autobuy`
appear under both Weapons and Menus: 9 fixes in the first pass; round 2
in its own section below). Round 2 split three Menus rows (what was
fixed from what wasn't) and recounted HUD from its table.

## refcmp: the maps with reference captures

Reference captures exist for de_dust2 (32 views), de_nuke (27),
cs_office (23) and de_aztec (13) (`~/.local/share/mashup/dump/refcmp/`,
taken 2026-10-05/06); none for cs_italy, de_dust, de_inferno, de_train,
cs_assault, cs_compound, de_port or the mg_ maps. Ours were captured
again from this branch (`refcmp capture-ours --out <scratch>`, then
`report`). No HUD references are in the dump folder
(`refcmp/hud/ref/` is empty), so `hudcmp` couldn't run; the HUD's
measured rectangles live in `tests/it/heavy/hud_layout.rs` and pass.

| Map | Views | Mean abs diff (this branch) | Before (2026-10-07 captures) | Mean luma error | Worst three views |
|---|---|---|---|---|---|
| de_dust2 | 32 | 0.0308 | 0.0308 | 0.0029 | `sky_n` 0.060 (luma 0.753/0.743), `a_sign_e` 0.052, `a_sign_w` 0.047 |
| de_nuke | 27 | 0.0241 | 0.0250 | 0.0057 | `cam6` 0.043, `cam16` 0.041 (sharpness 13.0/10.8), `cam4` 0.034 |
| cs_office | 23 | 0.0185 | 0.0204 | 0.0019 | `cam0` 0.036, `zone1_180` 0.028, `zone1_90` 0.027 |
| de_aztec | 13 | 0.0406 | 0.0409 | 0.0015 | `zone0_270` 0.064, `zone0_180` 0.059, `zone1_90` 0.054 |

Reading: abs diff is the mean absolute difference per pixel (0-1);
luma is each image's mean. Brightness matches everywhere (mean luma
error under 0.006). What the worst views show (side-by-sides in the
report folders):

- de_nuke `cam16`, `cam6` (and every view with the 3D skybox in it): the
  far mountains of nuke's 3D skybox are fogged out to flat grey in ours
  and the cooling towers turn blue-grey, where CS:S shows the mountains
  plainly with a light haze. Ours divides the sky_camera's fog distances
  by its scale (nuke: 512-11000 over 16); dust2's scale is 4, so there it
  only thickens the haze a little. Tried here without the division
  (both `map::material_fog` and the sky camera's `DistanceFog`): nuke's
  mountains then match CS:S and nuke's mean falls 0.0241 -> 0.0238
  (`cam1` 0.027 -> 0.020, `cam4` 0.034 -> 0.027), but cs_office's skyline
  (its 3D skybox: scale 16, fog 0-15000) loses the haze CS:S has (`cam0`
  0.036 -> 0.041) and dust2 doesn't change. One rule doesn't fit both,
  so the division stays until capture C10 says what nuke's mountains are
  (3D geometry or the 2D sky `de_cobble_hdr` behind fully fogged
  geometry). Without the fog, nuke's cooling towers (`coolingtower.mdl`,
  a VertexLitGeneric static prop in the 3D skybox, no baked `.vhv`) show
  dark teal where CS:S's are light grey: their light probe in the
  skybox is wrong (a separate gap, listed under Rendering).
- de_aztec `zone0_*`: the difference is texture-level noise over the
  bump-mapped walls and the grass blend (luma equal to 0.001), plus
  the sky at the top: filtering, not light.
- de_dust2 `sky_*`, `a_sign_*`: the sky's rope and far hills (known:
  rope sag, haze) and the bump-mapped walls' detail.
- cs_office `cam0`: the intermission camera's snowy yard and fog.

## Movement

| Item | Status | Impact | Cost | Notes |
|---|---|---|---|---|
| Ground, air, duck, jump, bhop cap, stamina, ladders, water | have | | | movecmp to 0.002 units (OBSERVABILITY, movecmp) |
| Fall damage and landing roll | have | | | specs/cs_source/fall_damage.md |
| Surf ramps | differs | L | | tech-debt "cs_source movement": the rampbug patched out (our choice) |
| Slowdown when shot ("tagging") | missing | H | S | CS:S slows a player hit by a bullet for a moment; not in any spec. Capture C1 |
| View punch when hit (flinch) | missing | H | S | CS:S kicks the victim's view on damage; our punch is the landing roll only (tech-debt "Source movement"). Capture C1 |
| Scoped speeds (AWP, Scout, autos zoomed) | differs | M | S | spec movement.md Q1 |
| Duck spam limit, duck transition times | differs | M | S | spec movement.md Q6 |
| Backpedal speed (`sv_backspeed`) | differs | L | S | spec movement.md Q10 |
| Base velocity (conveyors, water currents) | differs | L | M | tech-debt "Source movement"; spec Q15 |
| Camera under a low ceiling | differs | M | | other work in flight (mg_jacks) |
| Tick rate 66.67 vs community 100 | differs | L | | spec Q17 |

## Weapons, recoil, spread

| Item | Status | Impact | Cost | Notes |
|---|---|---|---|---|
| Damage, falloff, hitgroups, armour | have | | | measured M5-M7 |
| AK-47/M4A1/pistols/AWP recoil, inaccuracy | have | | | measured M1-M4 |
| `invnext` / `invprev` (the mouse wheel's weapon commands) | have (fixed) | H | S | were missing; the default wheel binds stay `+jump` (mashup's choice; CS:S binds MWHEELUP invprev, MWHEELDOWN invnext) |
| Picking the second grenade from the HUD selection | have (fixed) | M | S | the pick drew the first weapon of the slot; now `Intent::pick` names it |
| `autobuy` / `rebuy` (F1 / F2) | have (fixed) | H | S | were missing (the buy menu's buttons greyed); details unmeasured (tech-debt), capture C4 |
| Spread generator | differs | L | M | tech-debt "CS:S's spread and recoil": seeded as CS:S, our generator (spec Q1) |
| Other automatics' recoil, scoped/silenced/burst | differs | M | S | spec weapons.md Q3 |
| Penetration materials other than wood/metal/sand/concrete | differs | M | S | tech-debt "Penetration"; spec Q7 |
| Unmeasured gun values (556/9mm penetration...) | differs | L | S | tech-debt "The CS:S guns" |
| Grenade throw timings, redraw, pin rules | differs | M | S | tech-debt "Grenades"; spec grenades.md Q1-Q5 |
| Same-type pickup takes clip and reserve | differs | L | S | tech-debt "Picking up a dropped gun" |
| `cl_autowepswitch` while firing | differs | L | S | tech-debt row |
| Knife idle timing | differs | L | | tech-debt "View-model idle" |
| Flashlight (`impulse 100`, F) | missing | L | M | `mp_flashlight` decides on a server; capture C9 for the default |
| Night vision (`nightvision`, N; buyable) | missing | L | M | the buy menu shows it greyed |
| Shield | n/a | | | CS:S has none |

## Sounds

| Item | Status | Impact | Cost | Notes |
|---|---|---|---|---|
| Weapon fire, reload parts, empty, deploy, zoom | have | | | spec sounds.md; reload parts at our times (Q9) |
| Footsteps, ladders, water, landing | have | | | spec; silence rules unmeasured (below) |
| Round announcer, radio, chat blip, bomb | have | | | |
| Hurt and death sounds | differs | H | S | tech-debt "Hurt and death sounds": rules ours; sound.md plan lists 7 captures |
| Weapon pickup and buy sounds | have (fixed) | H | S | `Player.PickupWeapon`, `BaseCombatCharacter.ItemPickup2` were never played; which plays when is inferred (tech-debt), capture C2 |
| HUD weapon selection sounds | have (fixed) | H | S | `Player.WeaponSelectionMoveSlot`/`WeaponSelected`/`DenyWeaponSelection` were never played; when each plays is inferred, capture C3 |
| Under water muffle | have (fixed) | M | S | nothing changed with the head under water; now a steady low-pass (Source's `dsp_water` is engine-side: values ours, capture C5) |
| Distance falloff curves | differs | H | S | spec sounds.md Q1 (H1/H2 hypotheses) |
| Footstep silence rules, jump sound | differs | H | S | spec Q3, Q4 |
| Room DSP presets | differs | M | M | tech-debt "Room DSP": every value ours |
| `dsp_player`, `soundmixer` | missing | M | M | read, not played (spec Q14) |
| Wave choice (repeat avoidance) | differs | L | S | spec Q2 |
| ambient_generic levels | differs | L | S | spec Q11 |
| Scrapes | differs | L | S | spec Q8 |
| Ricochets (`FX_RicochetSound.Ricochet`) | missing | M | S | never played; spec sounds.md Q7 |
| Drowning (`Player.DrownStart`/`DrownContinue`, damage) | missing | L | S | no drowning at all; capture C6 |
| Hint sound (`Hud.Hint`) with `cl_autohelp` hints | missing | M | M | hints not in (ui-parity.md, greyed) |
| Spray sound, night vision and flashlight sounds | missing | L | | their features are missing |
| AK full-auto choppy | | | | other work in flight |

## HUD

| Item | Status | Impact | Cost | Notes |
|---|---|---|---|---|
| Health, armour, ammo, money, round timer | have | | | hudcmp (rectangles in `heavy::hud_layout`) |
| Weapon selection, pickup history | have | | | hudcmp |
| Death notices, scoreboard, chat, radio menus | have | | | |
| Bomb/kit icons, progress bar, radar dots | have | | | |
| Target ID | have | | | range a guess (tech-debt) |
| Flash white-out, smoke grey | have | | | |
| Crosshair and its options | have | | | ranges unmeasured (ui-parity.md) |
| Spectator bars, freeze cam | have | | | |
| Team menu, buy menu (VGUI) | have | | | |
| Round result | have (fixed) | H | M | the win panel from the install (`resource/ui/win_round.res` placed by `WinPanel_Round`, its `winpanel_*`/`funfact_*` strings): winner, reason, round time, MVP, a fun fact, over the network too (`client::win_panel`); the centre print only without the layout. MVP by CS:S's published award reasons; which fun fact, the title's colour, how long it shows and whether a centre print shows with it are ours (C7, C12) |
| Damage direction indicator | have (fixed) | H | M | was the CS 1.6 `pain_*` arcs around the crosshair and only where the damage was dealt (never on a network client); now `HudDamageIndicator`'s red side bars (hudlayout.res: `dmg_xpos` 30, `dmg_ypos` 100, `dmg_wide` 36, `dmg_tall1` 240, `dmg_tall2` 200) from damage that reached the health, sent to the victim's client (`net::Hurt`). Which bar for hits from in front/behind (both), the fade (1 s) and alpha are ours: C8 |
| Radar look (zoom, translucency, range) | differs | M | S | backlog 2b: ours; `cl_radaralpha` (CS:S's 200) now sets the map's opacity |
| Scoreboard row spacing, dead rows | differs | M | S | tech-debt "Game-look HUD parts" |
| Spectator menu look | differs | L | S | backlog 2b (`bottomspectator.res`) |
| Freeze cam panel (`freezepanel_basic.res`) | differs | M | S | backlog 2b |
| Map time left on the scoreboard | have (fixed) | L | S | beside the clock icon (`WinConditionLabel`) with `mp_timelimit`; a client has the map's start in `NetRound` |
| Class menu (`classmenu_*.res`) | have (fixed) | M | M | after picking a team; the eight classes' models (`models/player/*.mdl`) on bodies, auto-select a random one |
| MOTD on joining (`motd.res`, `cfg/motd_default.txt`) | have (fixed) | L | S | then the team menu, then the class menu; HTML shown as its text (we don't render HTML) |
| Hint panel messages (`cl_autohelp`) | missing | M | M | |

## Menus and commands

The install's `cfg/config_default.cfg` binds (a fresh CS:S config) and
common console commands, against mashup's.

| Item | Status | Impact | Cost | Notes |
|---|---|---|---|---|
| Main menu, Options (six tabs), Create Server, Find Servers | have | | | docs/plans/active/ui-parity.md |
| Movement, attack, slots 1-5, `lastinv`, `drop`, `buymenu`, `chooseteam`, radio, chat, `buyammo1/2`, `+showscores` | have | | | same keys as CS:S |
| `autobuy` (F1), `rebuy` (F2) | have (fixed) | H | S | commands added; F2 stays mashup's debug UI and F1 the inspector's mouse release, so the F-key binds aren't added (bind them in Options) |
| `invnext`, `invprev` | have (fixed) | H | S | see Weapons |
| `jpeg` (F5) | have (fixed) | L | S | a PNG in the screenshots folder named `<map>NNNN` |
| `kill`, `explode`, `retry`, `connect`, `status`, `exec`, `alias`, `bind`, `toggle`, `incrementvar`, `net_graph`, `cl_showfps`, `cl_showpos` | have | | | |
| Default binds | differs | M | | wheel jumps, V noclip, F2 debug UI, Alt free look (mashup's); `config_default.cfg`'s F, N, T, K, H, I, J, O, F10, PAUSE binds have nothing to run |
| Console | differs | L | M | a drop-down, not CS:S's VGUI frame (ui-parity.md) |
| `con_enable` and `volume` defaults | differs | L | | ui-parity.md: undecided |
| Options apply at once | differs | L | | ui-parity.md: deliberate |
| `impulse 100` (flashlight), `nightvision` | missing | L | M | |
| `impulse 201` (spray), Import Spray | missing | M | M | |
| `+voicerecord` (voice chat), Voice tab | missing | M | L | players use Discord meanwhile |
| `record`, `stop`, `playdemo`, `demoui` | missing | L | L | no demos |
| `fps_max` | have (fixed) | L | S | 300 as CS:S (`client::perf::FrameLimit`); off under `--bench` |
| `timeleft` | have (fixed) | L | S | with `mp_timelimit` |
| `users`, `ping` | have (fixed) | L | S | Source's layouts (`net::users_text`, `ping_text`) |
| `motd` | have (fixed) | L | S | opens the MOTD panel |
| `buyequip`, `cancelselect` | have (fixed) | L | S | the buy menu's equipment page; close the weapon selection |
| `commandmenu`, `showbriefing`, `cheer` | missing | L | S | what each shows in CS:S is unclear from the data (no capture planned) |
| `cl_radaralpha`, `hud_saytext_time` | have (fixed) | L | S | 200 and 12 s, as CS:S |
| `cl_minmodels` | missing | L | S | needs player models per class (class menu) |
| Player list (mute), stats, achievements dialogs | missing | L | M | |

## Rendering, lighting, materials

| Item | Status | Impact | Cost | Notes |
|---|---|---|---|---|
| Lightmaps, bump, envmaps, two-texture blend, sky, fog | have | | | refcmp above |
| Decals, overlays, impact effects, tracers | have | | | specs; the "placeholder" tech-debt row was stale and is gone |
| Ropes, sprites, dust motes, beams | have | | | specs |
| View models | have | | | view-models.md |
| HDR (`mat_hdr_level 2`, CS:S's default) | differs | M | M | tech-debt "Lightmaps": stand-in exposure and bloom; references are LDR |
| Player shadows | differs | M | | blob only (tech-debt "Dynamic shadows"); decals under shadows and steadier shadows merged from main 2026-10-13 |
| World reflections | differs | L | S | tech-debt: top mip only, highlight narrower |
| Water refraction | differs | L | M | tech-debt "Water" |
| Particles blend in linear space | differs | L | M | tech-debt |
| Glow occlusion by rays | differs | L | M | tech-debt |
| LOD switch metric, no hysteresis | differs | L | S | tech-debt "Levels of detail" |
| Rope wind sway | differs | L | S | tech-debt "Ropes" |
| Detail props layout | differs | L | S | tech-debt "Detail props" |
| Player animation (feet slide, no death layer) | differs | M | M | tech-debt "Player animation": SDK template speeds |
| Ragdoll push per weapon | differs | M | S | tech-debt "Ragdolls" |
| Muzzle flash look, view model lighting | differs | M | S | tech-debt "View models" |
| Prop light probes | differs | L | M | tech-debt |
| 3D skybox haze, sky down face | differs | L | S | refcmp `sky_*` worst views; capture listed in backlog |
| 3D skybox fog distances (nuke's far mountains fogged out) | differs | M | S | refcmp above: nuke prefers the fog undivided by the sky_camera's scale, office divided; capture C10 |
| Static props lit in the 3D skybox (nuke's cooling towers teal, CS:S light grey) | differs | M | M | seen once the sky fog is off; their light probe (no `.vhv`); capture C10 |
| Blood decals on characters | missing | M | M | backlog 3 "Decals, remaining" |
| Motion blur, colour correction options | missing | L | M | video-settings.md (greyed) |

## Maps and entities

| Item | Status | Impact | Cost | Notes |
|---|---|---|---|---|
| Entity I/O, triggers, movers, breakables, doors, fire | have | | | specs/source |
| Soundscapes | have | | | |
| Stock objectives (bomb, hostages) | have | | | hypotheses: tech-debt "The bomb and hostages" |
| Props collide by their visible mesh, not `.phy` | differs | M | M | tech-debt |
| Pushers (no bevels, no players or props pushed) | differs | L | M | tech-debt "Pushers" |
| Round restart keep-list | differs | M | S | entity_io.md Q1 |
| Physics shadow controller | differs | L | M | tech-debt |
| Prop damage left-outs | differs | L | S | tech-debt |
| Steam heat-wave jets, func_dustcloud | missing | L | S | tech-debt |
| Beams' noise and scroll | differs | L | S | tech-debt "Beams" |
| Community-map entity readings | differs | M | | tech-debt rows "Community-map", "Minigame flows" |
| Water and physics (buoyancy) | differs | L | M | tech-debt |
| LOD and fading of props (`fademindist`) | differs | L | S | performance backlog |
| Model doors beyond the spec | differs | L | S | tech-debt |
| Buy zones round spawns on maps without | have | | | merged from main 2026-10-13 |
| `info_map_parameters` buying | have | | | |

## Rules and economy

| Item | Status | Impact | Cost | Notes |
|---|---|---|---|---|
| Rounds, freeze, buy time, money table | have | | | rounds.md; defaults unmeasured (rounds.md Next 1) |
| Kill rewards, team-kill penalty | have | | | |
| Survivors keep weapons | have | | | |
| Spectators | have | | | |
| `mp_timelimit`, `mp_maxrounds`, `mp_winlimit`, `mp_chattime`, map cycle | have (fixed) | M | M | were missing (a session never left its map; CS:S's Create Server sets a 20-minute limit, `cfg/settings_default.scr`); now the intermission and the next map of the install's `cfg/mapcycle_default.txt`; details unmeasured (tech-debt), capture C9 |
| Team balance (`mp_autoteambalance`, `mp_limitteams`) | have (fixed) | M | S | `mp_limitteams` was in; `mp_autoteambalance` (1) moves players as the next round starts (`rules::balance_teams`); who moves and the warning unmeasured: C11 |
| `mp_tkpunish`, `mp_fadetoblack` | have (fixed) | L | S | both 0 as CS:S; a team killer dies as the next round starts; the dead fade to black over 1 s (ours) |
| Ammo prices and sizes | differs | L | | tech-debt "Ammo buying": community values |
| Bomb and hostage details | differs | M | | tech-debt; objectives.md open questions |
| Draw rules on one-sided maps | differs | L | | tech-debt "Minigame flows" |
| Freeze-time reload | differs | L | | backlog: capture to confirm |

## Networking feel

| Item | Status | Impact | Cost | Notes |
|---|---|---|---|---|
| Prediction, interpolation, lag compensation | have | | | multiplayer.md |
| Server browser, LAN, passwords, downloads | have | | | |
| Chat and radio over the network | have | | | |
| `cl_interp`, `cl_updaterate`, `rate`, `cl_lagcompensation` | have | | | |
| Movers mispredicted for a round trip | differs | L | M | tech-debt "The network so far" |
| `cl_cmdrate` | differs | L | S | missing as a cvar (one command per tick) |
| Voice chat | missing | M | L | |
| Sprays | missing | M | M | |
| Demos | missing | L | L | |
| Votes (kick, map) | missing | L | M | |

## Fixed in this pass

- `invnext` / `invprev`: step through every weapon held, slot by slot
  and wrapping, through the HUD selection with `hud_fastswitch 0` (a
  move sound) or straight away with `1` (`client::weapon_select`);
  bindable in Options' keyboard tab (no longer greyed).
  Tests: `weapon_select::invnext_and_invprev_step_through_every_weapon_by_slot`,
  `client::weapon_select` unit tests.
- A pick from the HUD selection names its weapon's place in the slot
  (`core::Intent::pick`), so the second grenade can be drawn (it drew
  the first). Test: `weapon_select::a_pick_draws_the_second_weapon_in_a_shared_slot`.
- The selection's sounds: moving (and opening), picking, an empty slot.
- Buying a weapon or picking one up plays `Player.PickupWeapon`;
  buying armour or a kit `BaseCombatCharacter.ItemPickup2` (ammo kept
  `BaseCombatCharacter.AmmoPickup`). Test:
  `weapon_select::buying_and_picking_up_play_the_pickup_sounds`.
- `autobuy` and `rebuy` (and the buy menu's buttons for them):
  `cl_autobuy` / `cl_rebuy` userinfo lists, CS:S's default files'
  entries by default. Test:
  `rounds::autobuy_takes_the_list_and_rebuy_restores_the_last_buy`.
- Under water everything is muffled (`map::hearing`). Test:
  `map::hearing` unit test.
- `jpeg`: a screenshot named after the map. Test: `client::capture`
  unit test.
- The end of a map's game: `mp_timelimit`, `mp_maxrounds`,
  `mp_winlimit`, the intermission (`mp_chattime`, everyone held, the
  scoreboard up, over the network too) and the next map of the
  install's map cycle; `timeleft` (`rules::game_end`). Tests:
  `rounds::the_game_ends_after_mp_maxrounds_with_an_intermission`,
  `rounds::the_time_limit_ends_a_deathmatch_game`, `rules::game_end`
  and `games::cs_source` unit tests.
- tech-debt: the stale "Shot effects are placeholders" row removed.

## Fixed in round 2 (weapons, sounds, HUD, menus, rules, movement, networking)

- The round's end shows the game's win panel (`client::win_panel`,
  `resource/ui/win_round.res` at the HUD layout's `WinPanel_Round`):
  "Terrorists Win", the reason (`winpanel_end_*`), the round's live time,
  "MVP: X for most kills" (or planting, defusing, the last rescue:
  `rules::rounds::Mvp`) and a fun fact (`funfact_*`, `RoundSummary`,
  counted by `rules::rounds::RoundStats`); a client gets it in
  `net::RoundOver` (now entity-mapped). The centre print stays only
  without the layout. Tests: `objectives::*` (planter, defuser, no MVP),
  `net_rules::a_full_round_with_two_clients`, `client::win_panel` unit
  test; screenshot checked on de_dust2.
- The damage indicator is the HUD layout's `HudDamageIndicator`: red
  bars at the screen's sides (`client::hud_sprites::DamageBars`), lit by
  `core::DamageTaken` (damage that reached the health), which a network
  client now gets for its own player (`net::Hurt`; before, a client never
  saw the indicator). Tests:
  `net_weapons::the_player_hit_is_told_which_way_the_damage_came`,
  `client::hud_sprites` unit tests; screenshot checked.
- The scoreboard shows the map's time left with `mp_timelimit`
  (`NetRound::map_started` for clients). Test: `client::scoreboard` unit
  test; screenshot checked.
- `mp_autoteambalance` (`rules::balance_teams`), `mp_tkpunish`,
  `mp_fadetoblack` (`client::senses`). Tests:
  `rounds::uneven_teams_are_balanced_as_the_next_round_starts`,
  `rounds::tkpunish_kills_team_killers_as_the_next_round_starts`,
  `client::senses` unit test.
- Commands and cvars: `fps_max` (`client::perf::FrameLimit`), `ping`,
  `users`, `buyequip`, `cancelselect`, `cl_radaralpha`,
  `hud_saytext_time`. Tests: unit tests in `client::perf`, `net`,
  `client::weapon_select`, `client::chat`.
- `NET_VERSION` net16 (`Hurt`, `RoundOver`'s summary, `NetRound`'s map
  start).

## Fixed: the join flow

- Joining a server, or starting a game with `map` (the menus' Create
  Server, `--map`), shows CS:S's join flow (`client::join_menus`): the
  message of the day in `resource/ui/motd.res` (the server's name, its
  `motd.txt`: the host's config folder, else the install's `motd.txt`,
  `cfg/motd.txt`, `cfg/motd_default.txt`; sent to clients as they are
  let in, `net::ServerMotd`), OK opens the team menu, a team opens the
  class menu (`classmenu_ter.res` / `classmenu_ct.res`, each class's
  picture beside it), and the player spawns as the round rules allow
  (in the freeze time, or with nobody of its team alive, this round;
  else the next). Until it picks a team it is on none
  (`core::Unassigned`: no body, watching as a spectator); on a team
  without a class (`core::ChoosingClass`) it is neither spawned nor
  counted for the round's end. The chat says "Player X has joined the
  game" as it connects and "X is joining the Terrorist force" when it
  picks. `motd` shows the message again.
- Differences: we don't render HTML: an HTML message shows its text
  (tags out, line breaks kept); `cl_disablehtmlmotd 1` (now in
  Multiplayer > Advanced) shows the server's plain-text one
  (`motd_text.txt`, else `cfg/motd_text_default.txt`), as CS:S does. A
  message that is a web address shows the address. Bots keep their
  team's first model (bots are parked). Players keep their team over a
  `changelevel` (CS:S's behaviour there is unmeasured).
- `joinclass <0-8>` (`core::PlayerClass`, replicated; 0 auto-selects one
  of the team's at random); `jointeam 0` auto-assigns. The class's model
  (Phoenix Connexion, Elite Crew, Arctic Avengers, Guerilla Warfare; SEAL
  Team 6, GSG-9, SAS, GIGN) is the body others see and its hitboxes.
- Scripted runs skip it (`rules::JoinChoice`, `--join menus|auto|1|2|3`):
  `--screenshot`, `--views`, `--spawn`, `--look`, `+jointeam <n>` and
  `+spectate` join at once; tests' `Sim`/`NetSim` too (a network client
  says which in its userinfo, `_jointeam`). `bot_join_after_player`
  waits for a player on a team.
- `NET_VERSION` net17 (`ClassRequest`, `ServerMotd`, replicated
  `Unassigned`, `ChoosingClass`, `PlayerClass`). Tests: `join_flow::*`,
  `client::join_menus`, `net::motd` and `client` unit tests.

## Next fixes that need no capture

In order of impact over cost:

1. Hint messages (`cl_autohelp`, `HudHintDisplay`, the game's `#Hint_*`
   strings, `Hud.Hint`): when each shows is game logic not in the data;
   a spec session or capture first (M).
2. `cl_cmdrate` (CS:S's default 30 sends commands in fewer packets than
   ours, one a frame): a choice for the network feel, not a gap players
   see; left (S).
3. Ricochet sounds once spec Q7 says when they play (spec session).

## Captures wanted

For the coordinator, on the reference client (listen server, de_dust2
unless named; `sv_cheats 1`; bots with `bot_stop 1` as targets). Each
says what decides the open point.

- **C1 Hit slowdown and flinch.** Probe server (movecmp harness): a bot
  running forward at full speed (knife) is shot once in the chest and
  once in the leg by a second bot with a USP and an AK-47; log the
  victim's horizontal speed and view angles every tick for 1 s after
  each hit. Decides: the speed factor and its recovery, and the punch
  (pitch/yaw, size, decay) by hitgroup and weapon.
- **C2 Pickup and buy sounds.** `sv_soundemitter_trace 1` on the
  listen server: buy an AK-47, kevlar, kevlar+helmet, a defuse kit, an
  HE grenade and ammo; drop the AK and walk over it; touch a dropped AK
  while carrying one (ammo only). Decides which entry each emits
  (`Player.PickupWeapon`, `BaseCombatCharacter.ItemPickup2`,
  `BaseCombatCharacter.AmmoPickup`, none), and whether the round's
  starting pistol plays one at spawn.
- **C3 Weapon selection sounds.** With `hud_fastswitch 0`: press 2,
  then 2 again, then 4 with no grenade, then the wheel twice, then
  attack; again with `hud_fastswitch 1`. Record the client's audio (or
  `snd_show 1`): which of MoveSlot / Selected / Open / Close / Deny
  plays at each step.
- **C4 Autobuy and rebuy.** As a CT with $16000 and an AWP in hand,
  run `autobuy` (does it drop the AWP for the M4A1?); then with $16000,
  `buy ak47` as a T, `buy vesthelm`, `buy flashbang` twice, die, set
  money to $2000 and `rebuy` (what is bought, in what order), then
  `rebuy` again next round with $16000 (does the second rebuy remember
  the first snapshot or what the short round bought?). Also the
  console text each prints.
- **C5 Under water.** Record the client's audio standing in de_aztec's
  water with the head under while a bot fires an AK-47 at 500 units,
  and the same with the head above. Decides the muffle (cutoff, level,
  any reverb) to replace `map::hearing::UNDERWATER`.
- **C6 Drowning.** Stay under water in de_aztec's river (`noclip` off,
  head under) for 30 s with `sv_soundemitter_trace 1` and `developer
  1`: when damage starts, how much per hit and how often, the sounds,
  and whether health comes back after surfacing.
- **C7 Round win panel.** Screenshots at 1280x720 of a round's end by
  elimination, by bomb, by defuse and by time: the panel's place and
  text, how long it stays, whether a centre print shows too.
- **C8 Damage indicator.** Screenshots 0.1, 0.5 and 1 s after being
  shot from the left, right, front and behind (a bot with `bot_stop 1`
  turned toward you by `setang`, `ent_fire !picker` with a USP, or a
  second client): the arcs' place, colour and fade.
- **C10 3D skybox fog and lighting (de_nuke, cs_office).** On the
  refcmp views `cam16` and `cam2` (de_nuke) and `cam0` (cs_office):
  captures with `r_3dsky 0`, with `fog_override 1; fog_enableskybox 0`,
  and as they are. Decides whether nuke's far mountains are 3D skybox
  geometry or the 2D sky, so whether the sky_camera's fog distances are
  in skybox or world units (`map::material_fog`); the cooling towers'
  colour without fog says what their light probe should give.
- **C9 Defaults.** `mp_flashlight`, `mp_timelimit`, `mp_maxrounds`,
  `mp_winlimit`, `mp_chattime`, `mp_autoteambalance`, `mp_limitteams`,
  `fps_max` values on a fresh listen server started from Create Server
  (RCON `cvarlist mp_`); and the map change at the time limit: does
  the round finish first, how long is the intermission, what shows.
- **C11 Team balance.** Listen server, `mp_autoteambalance 1`,
  `mp_limitteams 0`, `bot_join_team T; bot_add` four times and one CT
  bot; play a round to its end. Screenshots and `developer 1` console
  text at the round's end and the next start: does "*** Auto-Team
  Balance next round ***" show (where), which bots move (note each
  one's score and join order first: the top scorer, the lowest, the
  newest?), at the end or the next round's start, and is a moved player
  respawned at once or kept dead.
- **C12 Win panel contents.** Ten rounds on de_dust2 with bots
  (`bot_quota 9`): screenshot each round's win panel at 1 s and 4 s
  after the end, with the round's kills from the console (`developer
  1`) and the scoreboard: which fun fact shows given the round's events,
  who is MVP on a time-out win and with tied kills, the title's colour
  for CTs and for a draw, and whether the panel goes when the next
  round starts or before.
