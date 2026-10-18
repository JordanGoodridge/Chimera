"""A player walking into a prop_physics (2026-10-18; specs/cs_source/
physics_props.md Q7, Q12): the probe bot runs (forward move 400, or any
other forward speed) into a wooden crate on de_dust2's flat tile floor
(physmeas.FLAT) made heavier with `massscale`; prints the bot's speed per
tick and the crate's track. Needs mashup_probe, mashup_props, mashup_ents.

    python3 tools/css_probe/pushmeas.py <mass kg> [forward move] [ticks]
"""

import os
import sys
import time

sys.path.insert(0, os.path.dirname(__file__))
import impacts  # noqa: E402
import physmeas  # noqa: E402
from physmeas import rcon  # noqa: E402

CRATE = "models/props_junk/wood_crate001a.mdl"  # 30 kg, 40 units


def push(mass, fwd=400.0, ticks=200, model=CRATE, base=30.0, gap=60.0):
    x, y, _ = physmeas.FLAT
    x -= 20.0
    e = physmeas.spawn(model, x + gap + 16 + 20, y, 20.5, 0, 0, "prop_physics", "massscale", str(mass / base))
    time.sleep(1.5)
    rcon("mashup_hp 1 100")
    rcon(f"mashup_htrack {e} {physmeas.TRACK}")
    rows = impacts.run([f"0 {fwd} 0 0 0"] * ticks, (x, y, 0.05, 0.0), ticks * 0.015 + 1.0)()
    rcon("mashup_htrack -1")
    with open(os.path.join(physmeas.DS, physmeas.TRACK)) as fh:
        track = [tuple(float(v) for v in ln.split()) for ln in fh if len(ln.split()) == 4]
    rcon(f"mashup_kill {e}")
    return rows, track


if __name__ == "__main__":
    mass = float(sys.argv[1])
    fwd = float(sys.argv[2]) if len(sys.argv) > 2 else 400.0
    ticks = int(sys.argv[3]) if len(sys.argv) > 3 else 200
    rows, track = push(mass, fwd, ticks)
    last = None
    for r in [r for r in rows if r[0] != "S"]:
        v = tuple(round(float(c), 1) for c in r[4:7])
        if v != last:
            print("tick", r[0], "pos", " ".join(r[1:4]), "vel", *v)
            last = v
    x0 = track[0][1]
    for i in range(0, len(track), 10):
        t = track[i]
        v = (track[min(i + 1, len(track) - 1)][1] - t[1]) / 0.015
        print("crate t %.3f dx %.2f vx %.1f" % (t[0] - track[0][0], t[1] - x0, v))
