# CS:S movement probe

A SourceMod plugin that replays per-tick inputs on a bot in a real CS:S
dedicated server and logs the bot's position, velocity, ground and duck
state after every tick. `cargo run --features dev,tools --bin movecmp` drives it
and compares the result with mashup's Source movement (see
docs/OBSERVABILITY.md).

Setup (once per machine; nothing here touches the CS:S game install):

1. Dedicated server, Steam app 232330, anonymous SteamCMD login:
   `steamcmd.sh +force_install_dir ~/games/css-ds +login anonymous +app_update 232330 validate +quit`
2. Metamod:Source 1.12 and SourceMod 1.12 (Linux builds from
   alliedmods.net) extracted into `~/games/css-ds/cstrike/`. The server is
   32-bit: delete `addons/metamod_x64.vdf`.
3. Compile and install the plugin:
   `cd ~/games/css-ds/cstrike/addons/sourcemod/scripting && ./spcomp <repo>/tools/css_probe/mashup_probe.sp -o ../plugins/mashup_probe.smx`
4. Copy `mashup_probe.cfg` (below) to `~/games/css-ds/cstrike/cfg/`.
5. In mashup.local.toml: `[games.cs_source_server]` `path = "~/games/css-ds"`.

movecmp starts the server itself (LAN only, 127.0.0.1:27030, `-insecure`)
when it isn't running, and stops it afterwards unless `--keep-running`.

`mashup_probe.cfg`:

```
sv_lan 1
sv_cheats 1
rcon_password mashup-probe
mp_freezetime 0
mp_roundtime 60
mp_ignore_round_win_conditions 1
mp_autoteambalance 0
mp_limitteams 0
mp_autokick 0
bot_join_after_player 0
bot_join_team ct
bot_zombie 1
bot_dont_shoot 1
bot_quota 1
bot_quota_mode normal
```

Notes:
- CS:S ignores `-tickrate`: the server always runs 0.015 s ticks.
- The plugin switches the bot to its knife (max speed 250) before a run,
  and on placement resets its move type, remembered ladder and jump
  stamina. Its log adds the water level and ladder state per tick.

## Fall damage

`fallmeas.py` (specs/cs_source/fall_damage.md) drops the probe bot onto the
CT spawn floor of de_dust2 with chosen downward speeds and prints fall
speed and damage per landing; `fallmeas.py armor` checks armour and death.
It uses `mashup_health <hp> [armor]` (respawns the probe bot if dead, sets
its health and armour) and `mashup_wrun`'s `player_hurt` rows.

`mashup_botcmd <userid> <command...>` runs a client command as a bot
(`buy ak47`, `drop`); with `sv_soundemitter_trace 1` it shows which sound
entries buying emits on the server (parity-gaps.md C2).

## Sounds

Every run also logs the sounds the run's bot emits, as `S tick channel
volume level pitch flags sample` rows (a SourceMod normal-sound hook), e.g.
footstep bands and volumes (specs/cs_source/sounds.md "Footsteps").

## Weapon measurements

Being hit (M18 in specs/cs_source/weapons.md): `hitmeas.py <weapon> <aim
height> [armor]` uses `mashup_shooter x y z weapon tick aimz` (the first
other living player stands there with only that gun, turns to the run's bot
and fires once at that run tick) and prints the victim's speed modifier and
view punch after the hit. Needs a second bot (it adds a terrorist).

`weapcmp.py` (driver, Python 3, no dependencies; `rcon.py` is a minimal
Source RCON client) and `weapmeas.py` (one function per measurement) produce
the numbers in specs/cs_source/weapons.md, "CS:S values (measured)":

```
python3 tools/css_probe/weapcmp.py knife knife_range
python3 -c "import sys; sys.path.insert(0, 'tools/css_probe'); import weapmeas; weapmeas.timing('weapon_ak47')"
```

Plugin commands they use (all server commands, see the plugin header):
`mashup_wrun` (a run that keeps the bot's weapon and logs a `W` row with the
weapon state plus `E` rows for weapon events every tick; an optional sixth
input column selects weapon slot n-1 that tick), `mashup_give <weapon> [clip]
[reserve]`, `mashup_target x y z yaw health armor helmet hover` (the second
player; it is held at that yaw and its health/armour are restored after every
hit), `mashup_targetn n x y z yaw hover` (extra targets 1–3, other players
held the same way; needs that many more bots, e.g. `bot_add_t`),
`mashup_trace x y z pitch yaw [ignore_shooter]` (bullet-mask trace with
hitgroup), `mashup_wall x y z pitch yaw` (first surface along a ray, where
the ray leaves it again, thickness, surface props of both faces and the free
distance behind; players ignored), `mashup_walls` (thin walls seen from a
grid of ground points), `mashup_scan` (finds long clear sightlines) and
`mashup_info`.

Penetration helpers: `surfprops.py` maps a trace's surface props index to the
surface name and game material letter (reads `scripts/surfaceproperties*.txt`
from the server install through `vpkls.py`, a minimal VPK reader; nothing is
extracted into the repo). In weapmeas.py: `pen_series`/`pen_shot` (damage
behind a layer), `wall_pass` + `aim_catalog` (pass/fail by thickness),
`collateral`, `multi`, `reach_ground` (players and walls on one line),
`recoil_taps`, `recoil_state`, `kick_fit`, `kick_vs_speed` (recoil);
`passmap.py` + `passmap_summary.py` (pass/fail by thickness from one spot).

Notes:
- The bot AI sets the bot's view angles and weapon on its own; weapon runs
  force `v_angle` to the input angles each command and block AI weapon
  selection, but the AI still switches back to its best gun after a forced
  switch to the knife and toggles silencers. Deploy timing is therefore
  measured by giving the gun mid-run (`weapmeas.deploy`).
- A bot that joins mid-round waits dead; `mashup_target` respawns it.
- The first ~60 ticks after a run's start are unreliable for ducking (a
  duck held from the start took effect only after ~70 ticks, once not at
  all): settle with ~150 idle ticks before what's measured.
- Give a run a start position (`weapcmp.run(..., start=...)`): runs
  without one were seen to ignore the inputs' buttons (a reload pressed
  never happened), even with no freeze time.
- `movemeas.py` (scoped speeds, backpedal, duck timing), `recoilall.py`
  (recoil fits), `dropmeas.py` (drops) are the 2026-10-15 measurement
  scripts; airborne recoil used a jump before holding attack.
- With the server's `maxplayers 4` only three bots joined (the shooter and two
  targets).
- Afterwards restore the movement setup: `bot_kick`, `bot_join_team ct`,
  `bot_quota 1`, `mp_friendlyfire 0` (and `changelevel de_dust2` if the map
  was changed).

## Recording a SourceTV demo

For questions answered by what the server sends (entity props per tick,
game events, user messages, sounds; docs/OBSERVABILITY.md "CS:S demos"),
record a demo on the probe server over RCON:

1. `tv_enable 1`, `tv_delay 0`, `tv_transmitall 1` (all entities, not
   only near the camera), and `tv_snapshotrate 66` for a frame every tick
   (default 16 per second).
2. `changelevel <map>`: SourceTV starts with the next map load (it joins
   as a bot named SourceTV and takes a player slot).
3. `tv_record <name>`, play the situation, `tv_stoprecord`. The file lands
   in the server's `cstrike/<name>.dem`.
4. `cargo run --features tools --bin demodump -- <path>.dem --summary`.

Demos are game output: keep them out of the repo (e.g. under `target/`).
Afterwards `tv_enable 0` and a changelevel give the slot back.

## Round restart keep-list

`mashup_ents.sp` is a second plugin (compile it like `mashup_probe.sp`
and `sm plugins load mashup_ents`): `mashup_ents <file>` writes every
entity, networked or not, as `ref class hammerid targetname`.
`roundkeep.py out.json` changes to each stock map, lists the entities
before and after `mp_restartgame 1` and prints which classes were kept
(same reference) and which re-created (specs/source/entity_io.md
question 1; `world::ROUND_KEEP`).
