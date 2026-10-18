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
(`buy ak47`, `drop`, `noclip`, `kill`); `mashup_mt <userid> [1]` prints
that player's move type, life state, health, team and origin (1 first
respawns it with `CS_RespawnPlayer`; specs/cs_source/movement.md "Noclip
and spawning"); with `sv_soundemitter_trace 1` it shows which sound
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
view punch after the hit. Needs a second bot (it adds a terrorist). The
shooter lasts one run (or until `mashup_shooter -1`): while set, the
plugin strips that bot's own attack presses, so `mashup_fire` couldn't
make it shoot (plugins built before 2026-10-17 kept it forever: reload
the plugin or rebuild it).

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

The same plugin logs every logic_auto output as it fires to
`addons/sourcemod/logs/mashup_auto.log`. `mashup_hostages` prints each
hostage's index, health, origin, model and skin (objectives.md Q9);
hostage and round events (`hostage_follows`, `hostage_rescued`,
`hostage_rescued_all`, `hostage_killed`, `round_end` with winner and
reason, ...) go to `mashup_host.log` with the hostage's origin (Q11);
`mashup_norescue 1` removes every func_hostage_rescue as it spawns (then
`changelevel`: the map loads with none); a bot leads a hostage with
`mashup_input <hostage> Use "" <client>` (mashup_props). `mashup_hosthook` hooks every
hostage's TraceAttack and OnTakeDamage (each hit's damage before the
multiplier, hitbox, hitgroup, and the damage taken) into
`addons/sourcemod/logs/mashup_host.log` (objectives.md Q8); hostages are
re-created at every round start (`mp_restartgame` or a round end: new
references in the same edict slots), so hook again after one.
`mashup_htrack <entity> <file>` logs that entity's origin every server
frame (`game_time x y z`); `mashup_htrack -1` stops (objectives.md Q10).
It takes an edict index, so after a round start it follows the new
hostage in that slot (from its origin), not the old one; on cs_office
each slot also gets the same model again (four hostages, four models,
objectives.md Q9), which can look like a hostage kept.

## Prop damage, breakables, doors

`mashup_props.sp` is a third plugin (compile it like the others, `sm
plugins load mashup_props`). It logs to
`addons/sourcemod/logs/mashup_props.log`, one `t=<time> k=<tick> ...` line
per event: `dmg`/`post` rows (each hit's damage, damage type bits,
attacker, inflictor, damage force and position, health before and after;
`mashup_dmghook <class|index>` hooks the entities, `mashup_spawn` hooks what
it creates), every prop/breakable/door output as it fires (`out` rows) and,
with `mashup_sndlog 1`, every emitted sound (`snd` rows). Commands:
`mashup_spawn <class> <model|-> x y z pitch yaw [key value]...`,
`mashup_fire <userid> x y z pitch yaw <weapon> <buttons> <start tick>
<ticks>` (hold a bot there with only that weapon and press attack/attack2;
start at tick 100 or later: earlier presses were lost while the weapon
drew),
`mashup_hurt <victim> <attacker> <damage> <type bits> [x y z]`,
`mashup_input <index> <input> [parameter] [activator]`, `mashup_find
<class> [model part]`, `mashup_einfo`, `mashup_getprop <index|-1> <field>
[f]`, `mashup_watch <index> <frames>` (health, effects, solid flags every
frame, and when it is gone), `mashup_panes <index>` (a func_breakable_surf's
pane grid), `mashup_tp`, `mashup_hp <index> <health> [armor] [helmet]`,
`mashup_kill`. Every `player_hurt` is logged as a `hurt` row (health and
armour left, dmg_health, dmg_armor, hitgroup, weapon).

`propmeas.py` holds the 2026-10-17 measurements (specs/source/prop_damage.md,
breakables.md and doors_buttons.md open questions): `shot` (a fresh crate on
de_dust2's CT spawn shot by the bot), `gascan` (a gas can broken next to the
bot), `drop_on` (a prop thrown down onto the bot), `door` (a model door's
sounds through Open/Close/Lock/Use).

`hemeas.py` (grenades.md "Self damage", 2026-10-17): `python3
tools/css_probe/hemeas.py '{"vpos": [250, 2384, -126.2], "spos": [150,
2384, -126.2], "aimz": 0, "ff": 0, "reps": 2}'` throws an HE from the
second bot (`spos`) at the run bot's feet (`vpos`) and prints the
detonation point and each player's horizontal distance from it, the damage
before armour (`mashup_dmghook player` first) and the `hurt` row
(`t_armor`, `v_armor`: armour of the thrower and the run bot).

`physmeas.py` (physics_props.md section 7, 2026-10-18) spawns a prop on
de_dust2's flat concrete at (320, 1540, 0) and follows its origin every
server frame (`mashup_htrack`): `rest`, `drop` (bounce peaks), `slide`
(friction), `drag`, `sleep`. `overridescript surfaceprop,<name>` gives a
prop another material. The server's own `physics_debug_entity
<targetname>` prints a body's mass, inertia, velocity, damping, drag,
material and whether it is asleep; `physics_report_active` lists the awake
ones. `pushmeas.py <kg>` runs the probe bot into a crate made that heavy.
`phybox.py <model>` prints a model's `.phy` bounds, mass and surfaceprop.
Constraints (physics_constraints.md Q2-Q9) were spawned with `mashup_spawn
phys_hinge - x y z 0 0 attach1 <name> hingeaxis "x y z"` (the result is a
reference index `mashup_input` takes). `entsweep.py` reads the maps'
entity lumps (`class <classname> [keys]`, `count`, `outputs`; `MAPS=dir`
adds a folder, e.g. the content cache).

`impacts.py` (prop_damage.md 6): `drop <speed>` throws the 500 kg cupboard
(or a given model) down onto the bot while a `mashup_run` logs its velocity
(`HP=5000` for a bot that survives every contact, `HOOK=1` to hook its
damage once), `walk <250|130>` runs or walks the bot into a resting
prop_physics. `knifearmor.py <1 slash|2 stab> <armor>` knifes the first bot
from 40 units with a second (`bot_add_t`; `WPN=weapon_ak47` for a gun,
`HOLD=40` for a held attack) and prints the `hurt` row and the sounds.

`blastmeas.py` (grenades.md Q10, objectives.md Q3, 2026-10-18): `boom`
sets off an env_explosion (magnitude 100, radius 350: the HE's) at a
point while the run bot stands or crouches at another, optionally with a
motion-disabled prop between (`prop`), and prints the damage rows; `ent_fire`
does nothing over RCON, so a spawned logic_relay fires its Explode with a
delay. `he` is hemeas.py's throw with a crouching run bot, `bomb` has the
second bot (a terrorist) plant at de_dust2's B with `mp_c4timer 10` and
prints the blast's rows. The target point a blast aimed at is the damage
force's direction (`force=` in the row). `chordmeas.py <model> <weapon>
<yaw>...` floats a motion-disabled prop at the crouched shooter's eye,
turned so a level shot crosses it along a chosen chord, and prints the
chord and whether the target behind was hurt (weapons.md Q7: hollow
models such as de_tides' planter give chords the bullet doesn't cross
whole).

Notes: on cs_office the probe bot did not move at all (not even fall) in
`mashup_run` runs; de_dust2 works. Spawning a player exactly on a floor
(feet at its height) leaves it stuck: start 0.02 or more above.
