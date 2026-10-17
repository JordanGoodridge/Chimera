"""Prop/player impacts on the probe server (2026-10-17; specs/source/
prop_damage.md section 6, Q6 and Q13). Needs mashup_probe and mashup_props.

    python3 tools/css_probe/impacts.py drop <speed> [model] [mass key]
    python3 tools/css_probe/impacts.py walk <speed: 250 run | 130 walk> [model]

`drop` throws a prop_physics (default the 500 kg cupboard) straight down
onto the probe bot standing on de_dust2's CT spawn floor while a
`mashup_run` logs the bot's velocity every tick; prints the damage rows
(the bot's and the prop's) and the bot's velocity around the hit.
`walk` runs (or walks: the walk key) the bot into a prop_physics resting on
the floor 100 units ahead and prints the damage rows, the bot's speed per
tick and the prop's speed after.
"""

import os
import sys
import time

sys.path.insert(0, os.path.dirname(__file__))
import propmeas  # noqa: E402
from propmeas import SPOT, log_since, log_size, rcon, spawn  # noqa: E402

GAME = os.path.expanduser("~/games/css-ds/cstrike")
CUPBOARD = "models/props_c17/furniturecupboard001a.mdl"
CRATE = "models/props_junk/wood_crate001a.mdl"
IN_SPEED = 1 << 17


def run(lines, start, wait):
    """Write a mashup_run input (start line + per-tick lines), start it;
    returns a function reading the output rows once it is done."""
    with open(os.path.join(GAME, "impacts_in.txt"), "w") as f:
        f.write("start %.2f %.2f %.2f 0 %.1f 0 0 0\n" % start)
        f.write("\n".join(lines) + "\n")
    rcon("mashup_run impacts_in.txt impacts_out.txt")

    def rows():
        time.sleep(wait)
        with open(os.path.join(GAME, "impacts_out.txt")) as f:
            return [ln.split() for ln in f if ln.strip()]

    return rows


def drop(speed, model=CUPBOARD, height=120.0, hp=100, ticks=200, at_tick=60):
    x, y, z = SPOT
    rcon("mashup_hp 1 %d" % hp)
    if os.environ.get("HOOK"):
        rcon("mashup_dmghook 1")
    rows = run(["0 0 0 0 0"] * ticks, (x, y, z + 0.05, 0.0), ticks * 0.015 + 1.0)
    time.sleep(at_tick * 0.015)
    e = spawn(model, x, y, z + height, "prop_physics")
    pos = log_size()
    rcon(f"mashup_tp {e} {x} {y} {z + height} 0 0 {-speed}")
    got = rows()
    dmg = [r for r in log_since(pos) if " dmg " in r or " post " in r]
    info = rcon(f"mashup_einfo {e}").strip()
    rcon(f"mashup_kill {e}")
    return dmg, got, info


def walk(speed, model=CRATE, dist=100.0, ticks=120):
    x, y, z = SPOT
    rcon("mashup_hp 1 100")
    if os.environ.get("HOOK"):
        rcon("mashup_dmghook 1")
    e = spawn(model, x + dist, y, z + 20, "prop_physics")
    time.sleep(1.5)
    pos = log_size()
    buttons = IN_SPEED if speed < 200 else 0
    rows = run([f"{buttons} 400 0 0 0"] * ticks, (x - 60, y, z + 0.05, 0.0), ticks * 0.015 + 1.0)
    got = rows()
    dmg = [r for r in log_since(pos) if " dmg " in r or " post " in r or " out " in r]
    info = rcon(f"mashup_einfo {e}").strip()
    rcon(f"mashup_kill {e}")
    return dmg, got, info


def show(dmg, got, info):
    print("\n".join(dmg))
    print(info)
    last = None
    for r in [r for r in got if r[0] != "S"]:
        v = tuple(round(float(c), 1) for c in r[4:7])
        if v != last:
            print("tick", r[0], "pos", " ".join(r[1:4]), "vel", *v, "ground", r[7])
            last = v


if __name__ == "__main__":
    what = sys.argv[1]
    speed = float(sys.argv[2])
    model = sys.argv[3] if len(sys.argv) > 3 else None
    if what == "drop":
        show(*drop(speed, model or CUPBOARD, hp=int(os.environ.get("HP", "100"))))
    elif what == "walk":
        show(*walk(speed, model or CRATE))
    _ = propmeas
