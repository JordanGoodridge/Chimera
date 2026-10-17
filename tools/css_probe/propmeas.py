"""Prop and breakable damage measurements on the probe server (2026-10-17;
specs/source/prop_damage.md, breakables.md open questions). Needs the
mashup_props plugin (`sm plugins load mashup_props`).

    python3 tools/css_probe/propmeas.py shot <weapon> [buttons] [model]
    python3 tools/css_probe/propmeas.py log

`shot` spawns a fresh prop_physics_multiplayer (default the 30-health wooden
crate) on the CT spawn floor of de_dust2, makes the first bot fire at it
from 120 units with that weapon (buttons 1 attack, 2 attack2) and prints the
damage rows (damage, damage type bits, health before and after) and outputs.
"""

import os
import re
import sys
import time

sys.path.insert(0, os.path.dirname(__file__))
from rcon import Rcon  # noqa: E402

LOG = os.path.expanduser("~/games/css-ds/cstrike/addons/sourcemod/logs/mashup_props.log")
# de_dust2 CT spawn floor (z -126.24).
SPOT = (352.0, 2300.0, -126.0)

_r = None


def rcon(cmd):
    global _r
    if _r is None:
        _r = Rcon()
    return _r.exec(cmd)


def log_size():
    return os.path.getsize(LOG) if os.path.exists(LOG) else 0


def log_since(pos):
    with open(LOG) as fh:
        fh.seek(pos)
        return [ln.split("] ", 1)[-1].rstrip() for ln in fh]


def first_bot():
    for ln in rcon("status").splitlines():
        m = re.match(r"#\s+(\d+)\s+\"[^\"]+\"\s+BOT", ln)
        if m:
            return int(m.group(1))
    raise RuntimeError("no bot")


def spawn(model, x, y, z, cls="prop_physics_multiplayer", *kv):
    out = rcon(f"mashup_spawn {cls} {model} {x} {y} {z} 0 0 " + " ".join(kv))
    m = re.search(r"mashup_spawn: (\d+)", out)
    if not m:
        raise RuntimeError(out)
    return int(m.group(1))


def shot(weapon, buttons=1, model="models/props_junk/wood_crate001a.mdl", dist=120.0, pitch=20.0, start=80,
         ticks=1, wait=3.0):
    x, y, z = SPOT
    e = spawn(model, x, y, z + 19)
    time.sleep(0.5)
    pos = log_size()
    rcon(f"mashup_fire {first_bot()} {x} {y + dist} {z} {pitch} -90 {weapon} {buttons} {start} {ticks}")
    time.sleep(wait)
    rows = log_since(pos)
    print(rcon(f"mashup_einfo {e}").strip())
    rcon(f"mashup_kill {e}")
    return e, rows


if __name__ == "__main__":
    what = sys.argv[1]
    if what == "shot":
        args = sys.argv[2:]
        weapon = args[0]
        buttons = int(args[1]) if len(args) > 1 else 1
        model = args[2] if len(args) > 2 else "models/props_junk/wood_crate001a.mdl"
        _, rows = shot(weapon, buttons, model)
        print("\n".join(rows))
    elif what == "log":
        print("\n".join(log_since(max(0, log_size() - 4000))))


def gascan(d, model="models/props_junk/gascan001a.mdl", victim_client=1, wait=2.0, dz=0.0):
    """Break a gas can by a 100 bullet hit from the world with the bot
    standing d units away (x); prints the bot's damage rows."""
    x, y, z = SPOT
    rcon(f"mashup_hp {victim_client} 100")
    rcon(f"mashup_tp {victim_client} {x + d} {y} {z + dz} 0 0 0")
    e = spawn(model, x, y, z + 10)
    time.sleep(1.0)
    pos = log_size()
    rcon(f"mashup_hurt {e} 0 100 2")
    time.sleep(wait)
    rows = log_since(pos)
    return e, rows


def drop_on(speed, model="models/props_junk/wood_crate001a.mdl", height=110.0, client=1, dx=0.0, wait=1.5, hp=100):
    """Throw a prop straight down at `speed` onto the bot standing on the
    spot; prints the bot's damage rows."""
    x, y, z = SPOT
    rcon(f"mashup_hp {client} {hp}")
    rcon(f"mashup_tp {client} {x} {y} {z} 0 0 0")
    e = spawn(model, x + dx, y, z + height)
    pos = log_size()
    rcon(f"mashup_tp {e} {x + dx} {y} {z + height} 0 0 {-speed}")
    time.sleep(wait)
    rows = log_since(pos)
    info = rcon(f"mashup_einfo {e}").strip()
    rcon(f"mashup_kill {e}")
    return info, rows


def door(model="models/props_c17/door01_left.mdl", skin=0, hardware=1, extra=""):
    """A prop_door_rotating on the CT spawn floor: Open, Close, then Use
    while locked and unlocked by the first bot; prints the sounds it emits
    (doors_buttons.md Q8)."""
    x, y, z = SPOT
    rcon("mashup_sndlog 1")
    out = rcon(f"mashup_spawn prop_door_rotating {model} {x - 100} {y - 250} {z} 0 0 skin {skin} hardware {hardware} "
               f"speed 200 distance 90 returndelay -1 spawnflags 8192 {extra}")
    e = int(re.search(r"spawn: (\d+)", out).group(1))
    time.sleep(0.5)
    pos = log_size()
    for cmd, wait in (("Open", 1.5), ("Close", 1.5), ("Lock", 0.3), ("Use 1", 1.0), ("Unlock", 0.3), ("Use 1", 1.5),
                      ("Use 1", 1.5)):
        parts = cmd.split()
        if parts[0] == "Use":
            rcon(f"mashup_input {e} Use \"\" {parts[1]}")
        else:
            rcon(f"mashup_input {e} {parts[0]}")
        time.sleep(wait)
    rows = [r for r in log_since(pos) if f" {e} " in r]
    rcon("mashup_sndlog 0")
    rcon(f"mashup_kill {e}")
    return rows
