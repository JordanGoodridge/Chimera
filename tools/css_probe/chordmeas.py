"""How thick a layer of a prop's material a gun passes (weapons.md M13,
2026-10-18): a motion-disabled prop floats at the crouched shooter's eye
height on de_dust2's CT spawn floor, turned by `yaw`, so a level shot
crosses it along a chord set by the turn; the target hovers behind. Prints
the chord (`mashup_wall` along the shot) and whether the target was hurt.

    python3 tools/css_probe/chordmeas.py <model> <weapon> <yaw[:dz[:dx]]>...

(`dz`, `dx`: the prop moved up from the eye's height and sideways off the
shot line.)

Needs two bots (the target), mashup_props loaded.
"""

import os
import re
import sys
import time

sys.path.insert(0, os.path.dirname(__file__))
import weapcmp as W  # noqa: E402
import weapmeas as M  # noqa: E402

X, Y, FLOOR = 352.0, 2300.0, -126.2
BACK = 120.0  # shooter to the prop's origin
BEHIND = 50.0  # prop's origin to the target's chest


def eye():
    r0 = W.run("pen_eye", W.idle(30, 0, 90, buttons=W.IN_DUCK), start=(X, Y - BACK, FLOOR, 0, 90))
    return tuple(r0.w[30]["eye"])


def shot(model, weapon, yaw, e, dz=0.0, dx=0.0):
    z = e[2] + dz
    out = W.rcon(f"mashup_spawn prop_physics_multiplayer {model} {X + dx} {Y} {z} 0 {yaw} spawnflags 8")
    m = re.search(r"mashup_spawn: (-?\d+)", out)
    if not m:
        raise RuntimeError(out)
    p = int(m.group(1))
    time.sleep(0.3)
    wall = W.rcon(f"mashup_wall {e[0]} {e[1]} {e[2]} 0 90").strip()
    t = re.search(r"thick ([\d.]+) .* ent (-?\d+)", wall)
    chord, ent = (float(t.group(1)), int(t.group(2))) if t else (None, None)
    pt = (X, Y + BEHIND, e[2])
    r = M.pen_shot(weapon, (X, Y - BACK, FLOOR), pt, chest=55.0, eye=e)
    W.rcon(f"mashup_input {p} Kill")
    hurt = [h for h in r["hurt"]]
    return chord, ent == p, r["fired"], hurt


if __name__ == "__main__":
    model, weapon = sys.argv[1], sys.argv[2]
    e = eye()
    for a in sys.argv[3:]:
        yaw, dz, dx = (a.split(":") + ["0", "0"])[:3]
        chord, on_prop, fired, hurt = shot(model, weapon, float(yaw), e, float(dz or 0), float(dx or 0))
        print(f"{weapon} yaw {yaw} dz {dz or 0} dx {dx or 0}: chord {chord} (prop {on_prop}) fired {fired} hurt {hurt}", flush=True)
