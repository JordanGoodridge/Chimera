# Captures on the Windows PC (CS:S client)

What's left of [parity-gaps.md](parity-gaps.md) "Captures wanted" needs the
real CS:S client: screenshots, the client's own sounds, a human player.
Everything server-side is measured already (probe server, SourceTV demos).
This is a checklist for one or two sittings; each item names its capture
in parity-gaps.md, where the full description is.

Keep the files out of the repo (they are game output): put them in one
folder, e.g. `Documents\lp-captures\<date>\`, and send the folder (or a zip)
to the coordinator session. Name each file after its item (`C7_bomb_1s.png`).

Setup once: CS:S, windowed 1280x720 (`-windowed -w 1280 -h 720`), default
settings otherwise. Console on (`con_enable 1`). Screenshots with `jpeg` or
F5 land in `cstrike\screenshots\`.

## Sitting 1: a listen server with bots (about 45 min)

Create Server, de_dust2, 10 bots, `sv_cheats 1`, `mp_roundtime 2`.

1. **A POV demo for `demosim`** (most useful single item): `cl_cmdrate 66;
   cl_updaterate 66; record pov1`, play two rounds normally (walk, run, jump,
   crouch, shoot every gun you can, throw grenades), then `stop`. The file is
   `cstrike\pov1.dem`. A second one on a surf or bhop map if you play those.
2. **C7 round win panel** and **C12 win panel contents**: screenshots 1 s and
   4 s after each round end (elimination, bomb, defuse, time), with the
   scoreboard (Tab) once per round.
3. **C8 damage indicator**: `bot_stop 1`, get shot from left, right, front,
   behind (`setang` to turn); screenshots 0.1, 0.5 and 1 s after each hit.
4. **C3 weapon selection sounds**: record the game's audio (OBS or similar)
   with `hud_fastswitch 0`: press 2, 2 again, 4 with no grenade, the wheel
   twice, then attack; the same with `hud_fastswitch 1`.
5. **C4 autobuy and rebuy**: as described in parity-gaps.md C4, plus the
   console text each prints (copy the console or screenshot it).
6. **C11 team balance**: `mp_autoteambalance 1; mp_limitteams 0`, put four
   bots on T and yourself plus one bot on CT, play a round: who moves, the
   message, when.
7. **C15 muzzle flash**: in the lower tunnel, `host_timescale 0.1`,
   screenshots of the first two frames of an AK-47, USP, M3 and AWP shot.
8. **C23 sprays** and **C24 night vision**: as in parity-gaps.md.
9. **Empty gun with no reserve**: empty a pistol's last clip with no spare
   ammo (`give weapon_deagle`, fire everything, then drop to 0 reserve
   with `buy` disabled or after the buy time): does CS:S switch to another
   weapon by itself after the last shot, or stay on the empty gun
   clicking? The probe bot switched to its knife, but that may be its AI;
   ours stays on the empty gun.
10. **C25 votes** (needs a second human player on the listen server):
   `callvote` alone, a kick vote and a changelevel vote, F1 during a
   vote, a refused call; screenshots of each panel, as in parity-gaps.md
   C25.
11. **C26 automatic hints**: on a fresh profile with `cl_autohelp 1`
    (rename `cfg\config.cfg` first, put it back after), which hints
    appear when, as described in parity-gaps.md C26 (a video of the
    first two rounds is easiest; note each hint's time).
12. **C29 client props**: on cs_italy, knife and shoot a small cardboard
    box and a mug or bottle (count the hits each takes), with the game's
    audio recorded (any sound when a bottle breaks, beyond the bullet's?);
    on de_inferno, shoot a window (break sound or none?); then break a
    bottle and `mp_restartgame 1` (is it back?). As in parity-gaps.md
    C29.

## Sitting 2: sound (about 20 min)

Record the game's audio (OBS, 48 kHz) for:

1. **C5 under water**: in de_nuke's deep water (de_aztec's river is too
   shallow to go under), head under, a bot firing an AK-47 at about 500
   units; then the same with the head above.
2. **The planted bomb's beeps** (objectives.md Q4: the server sends none,
   the client makes them): plant on de_dust2 B (`mp_c4timer 45`), stand
   next to it and record until the explosion; the beep times give the
   schedule.
3. **Sound distance falloff** (parity-gaps "Distance falloff curves", the
   last high-impact gap): `bot_stop 1`, a bot firing an AK-47 from 250, 500,
   1000, 2000 and 3000 units in a straight open line (de_dust2 long A), you
   standing still facing it; say the distance into the mic or note the times.

## Sitting 3: refcmp references (automatic, about 30 min)

`scripts\playtest.ps1` builds the game; refcmp drives CS:S by itself
(docs/OBSERVABILITY.md, refcmp). Run the capture modes listed in C10, C17,
C18, C19, C20, C21, C22 and C27 (each names its views and settings). The output
folder is printed at the end: send it.

## What each sitting unlocks

- Sitting 1: movement and weapon checks against real human play (demosim),
  the HUD panels' look and timing, autobuy/rebuy rules, team balance, muzzle
  flash, sprays and night vision values.
- Sitting 2: the sound falloff curve (the last high-impact parity gap) and
  the underwater muffle.
- Sitting 3: HDR, 3D skybox fog, ropes, beams, sky, LOD, detail props and
  what cs_assault's PVS hides past sky brushes.
