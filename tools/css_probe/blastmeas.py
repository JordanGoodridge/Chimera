"""Blasts and flashes with things in the way (specs/cs_source/grenades.md
Q10, Q12, 2026-10-18).

    python3 tools/css_probe/blastmeas.py boom '{"at": [x, y, z], "victim": [x, y, z], "duck": 0}'
    python3 tools/css_probe/blastmeas.py he '{"vpos": [...], "spos": [...], "duck": 1}'
    python3 tools/css_probe/blastmeas.py flash '{"vpos": [...], "spos": [...], "yaw": 180}'

`boom` spawns an env_explosion (magnitude 100, radius 350) at `at` and
sets it off while the run bot stands (or crouches, `duck` 1) at `victim`;
prints the damage before armour (mashup_props' `dmg` rows) and the bell's
value for a body point 54 units over the feet. `he` is hemeas.py's throw
with the run bot crouching. `flash` has the second bot throw a flashbang
at the run bot's feet and prints the run bot's flash rows (`F`). `prop`
(a model, a position and `static`/`phys`) spawns something in between
first: `"prop": ["models/props/de_dust/du_crate_64x64.mdl", x, y, z]`
(a motion-disabled prop_physics_multiplayer; remove it with
`mashup_kill`-style input Kill afterwards, done here).

Needs mashup_props loaded and `mashup_dmghook player`; `he` and `flash`
need a second bot.
"""

import json
import math
import os
import re
import sys
import time

sys.path.insert(0, os.path.dirname(__file__))
import weapcmp as W  # noqa: E402
from propmeas import log_since, log_size  # noqa: E402

SIGMA = 350.0 / 3.0
IN_DUCK = 4


def bell(d):
    return 100.0 * math.exp(-d * d / (2 * SIGMA * SIGMA))


def spawn_prop(prop):
    model, x, y, z = prop[:4]
    yaw = prop[4] if len(prop) > 4 else 0
    out = W.rcon(f"mashup_spawn prop_physics_multiplayer {model} {x} {y} {z} 0 {yaw} spawnflags 8")
    m = re.search(r"mashup_spawn: (-?\d+)", out)
    if not m:
        raise RuntimeError(out)
    return int(m.group(1))


def kill(e):
    W.rcon(f"mashup_input {e} Kill")


def dmg_rows(pos):
    return [x for x in log_since(pos) if " dmg " in x or " hurt " in x]


def boom(at, victim, duck=0, prop=None, n=200, delay=2.0, settle=0.0):
    W.rcon("mashup_health 1000 0")
    e = spawn_prop(prop) if prop else None
    out = W.rcon(
        f"mashup_spawn env_explosion - {at[0]} {at[1]} {at[2]} 0 0 iMagnitude 100 iRadiusOverride 350 "
        "targetname mashup_boom spawnflags 0"
    )
    boom_e = int(re.search(r"mashup_spawn: (-?\d+)", out).group(1))
    # ent_fire does nothing over RCON: a relay sets it off `delay` later.
    out = W.rcon(f"mashup_spawn logic_relay - 0 0 0 0 0 OnTrigger mashup_boom,Explode,,{delay},-1 spawnflags 1")
    relay = int(re.search(r"mashup_spawn: (-?\d+)", out).group(1))
    time.sleep(settle)
    W.rcon(f"mashup_input {relay} Trigger")
    pos = log_size()
    buttons = IN_DUCK if duck else 0
    W.run("boom", [(buttons, 0.0, 0.0, 0.0, 0.0)] * n, start=(*victim, 0.0, 0.0))
    time.sleep(0.3)
    kill(boom_e)
    if e is not None:
        kill(e)
    rows = dmg_rows(pos)
    d = math.dist(at, (victim[0], victim[1], victim[2] + 54.0))
    return f"at={at} victim={victim} duck={duck} d54={d:.1f} bell={bell(d):.2f}\n  " + "\n  ".join(rows or ["no damage"])


def he(vpos, spos, duck=1, aimz=0, prop=None, n=200, tick=20):
    W.rcon("mp_friendlyfire 1")
    W.rcon("mashup_health 1000 0")
    W.give("weapon_knife")
    e = spawn_prop(prop) if prop else None
    out = W.rcon(f"mashup_shooter {spos[0]} {spos[1]} {spos[2]} weapon_hegrenade {tick} {aimz} 1")
    m = re.search(r"shooter (\d+)", out)
    sh = int(m.group(1)) if m else 2
    W.rcon(f"mashup_hp {sh} 1000 0 0")
    time.sleep(1.5)
    pos = log_size()
    buttons = IN_DUCK if duck else 0
    r = W.run("he", [(buttons, 0.0, 0.0, 0.0, 180.0)] * n, start=(*vpos, 0.0, 180.0))
    time.sleep(0.3)
    if e is not None:
        kill(e)
    g = [x for x in r.extra if x[0] == "G"]
    rows = dmg_rows(pos)
    b = [float(x) for x in g[0][3:6]] if g else None
    last = max(r.ticks) if r.ticks else None
    ducked = r.ticks[last] if last is not None else None
    return f"B={b} vpos={vpos} last tick row={ducked}\n  " + "\n  ".join(rows or ["no damage"])


def flash(vpos, spos, yaw=180.0, aimz=0, prop=None, n=200, tick=20, duck=0):
    W.rcon("mp_friendlyfire 1")
    W.rcon("mashup_health 1000 0")
    W.give("weapon_knife")
    e = spawn_prop(prop) if prop else None
    W.rcon(f"mashup_shooter {spos[0]} {spos[1]} {spos[2]} weapon_flashbang {tick} {aimz} 1")
    time.sleep(1.5)
    buttons = IN_DUCK if duck else 0
    r = W.run("fl", [(buttons, 0.0, 0.0, 0.0, yaw)] * n, start=(*vpos, 0.0, yaw))
    if e is not None:
        kill(e)
    # The detonation and the flash rows that changed after it.
    g = [x for x in r.extra if x[0] == "G"]
    out, last = [], None
    for x in r.extra:
        if x[0] == "F" and g and int(x[1]) >= int(g[0][1]) and x[2:4] != last:
            out.append(" ".join(map(str, x)))
            last = x[2:4]
    blind = [x for x in r.events if "blind" in " ".join(map(str, x))]
    rows = [" ".join(map(str, x)) for x in g] + out[:3] + [" ".join(map(str, x)) for x in blind]
    return "\n  ".join(rows) or "nothing"



def flash_thrower(vpos, spos, aimz=0, prop=None, n=200, tick=20):
    """The thrower's own blindness (`m_flFlashDuration`, read after the
    run): a prop between its eye and the detonation (`prop`) can shield
    it. The grenade is lobbed over the prop with `aimz`."""
    W.rcon("mashup_health 1000 0")
    W.give("weapon_knife")
    e = spawn_prop(prop) if prop else None
    out = W.rcon(f"mashup_shooter {spos[0]} {spos[1]} {spos[2]} weapon_flashbang {tick} {aimz} 1")
    m = re.search(r"shooter (\d+)", out)
    sh = int(m.group(1)) if m else 2
    time.sleep(1.5)
    r = W.run("flt", [(0, 0.0, 0.0, 0.0, 0.0)] * n, start=(*vpos, 0.0, 0.0))
    if e is not None:
        kill(e)
    g = [x for x in r.extra if x[0] == "G"]
    blind = [x for x in r.events if "blind" in " ".join(map(str, x))]
    dur = W.rcon(f"mashup_getprop {sh} m_flFlashDuration f").strip()
    return f"G={g[0][3:6] if g else None} thrower {sh}: {dur} blind={blind}"



def bomb(vpos, spos=(-1478.0, 2768.0, 4.0), duck=1, timer=10, n=1150, tick=20):
    """The second bot (a terrorist) plants the bomb at `spos` (de_dust2's
    site B by default; `mp_c4timer` `timer`) while the run bot stands or
    crouches at `vpos`; prints the blast's damage rows."""
    W.rcon(f"mp_c4timer {timer}")
    W.rcon("mashup_health 1000 0")
    W.give("weapon_knife")
    out = W.rcon(f"mashup_shooter {spos[0]} {spos[1]} {spos[2]} weapon_c4 {tick} 0 260")
    m = re.search(r"shooter (\d+)", out)
    sh = int(m.group(1)) if m else 2
    W.rcon(f"mashup_hp {sh} 1000 0 0")
    time.sleep(1.5)
    pos = log_size()
    r = W.run("bomb", [(IN_DUCK if duck else 0, 0.0, 0.0, 0.0, 0.0)] * n, start=(*vpos, 0.0, 0.0))
    time.sleep(0.3)
    ev = [e for e in r.events if e[1] in ("bomb_planted", "bomb_exploded")]
    return f"vpos={vpos} events={ev}\n  " + "\n  ".join(dmg_rows(pos) or ["no damage"])


def smoke_flash(vpos, spos, yaw=0.0, smoke=1, wait=3.0):
    """A smoke grenade (`smoke` 1) and then a flashbang thrown by the second
    bot at the run bot's feet, both landing about 40 units in front of it:
    the flash goes off inside the cloud. Prints the run bot's flash rows
    after the flash (compare `smoke` 0)."""
    if smoke:
        W.rcon("mashup_health 1000 0")
        W.give("weapon_knife")
        W.rcon(f"mashup_shooter {spos[0]} {spos[1]} {spos[2]} weapon_smokegrenade 20 0 1")
        time.sleep(1.5)
        r = W.run("sm", [(0, 0.0, 0.0, 0.0, yaw)] * 200, start=(*vpos, 0.0, yaw))
        g = [x for x in r.extra if x[0] == "G"]
        print("smoke", g[0][2:6] if g else None)
        time.sleep(wait)
    return "flash: " + flash(vpos, spos, yaw=yaw)


if __name__ == "__main__":
    what = sys.argv[1]
    a = json.loads(sys.argv[2]) if len(sys.argv) > 2 else {}
    reps = a.pop("reps", 1)
    for _ in range(reps):
        print({"boom": boom, "he": he, "flash": flash, "flash_thrower": flash_thrower, "bomb": bomb, "smoke_flash": smoke_flash}[what](**a))
