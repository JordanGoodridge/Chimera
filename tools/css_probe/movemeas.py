"""Movement open questions (specs/cs_source/movement.md Q1, Q6, Q10):
scoped top speeds, backpedal top speed, duck and unduck transition times and
what rapid duck presses do.

python3 tools/css_probe/movemeas.py [scoped|back|duck|all]
"""
import math
import os
import sys

sys.path.insert(0, os.path.dirname(__file__))
import weapmeas as M
from weapcmp import IN_ATTACK2, IN_DUCK, run, give, idle

YAW = 270.0
IN_FORWARD, IN_BACK, IN_MOVERIGHT = 1 << 3, 1 << 4, 1 << 10
# Columns of a tick row: tick x y z vx vy vz ground ducking box_top eye buttons water ladder.


def hspeed(v):
    return math.hypot(v[4], v[5])


def top_speed(r, first):
    return max(hspeed(r.ticks[t]) for t in r.ticks if t >= first)


def start():
    return (M.SPAWN[0], M.SPAWN[1], M.SPAWN[2], 0, YAW)


def scoped(weapons=("weapon_awp", "weapon_scout", "weapon_g3sg1", "weapon_sg550", "weapon_sg552", "weapon_aug")):
    for w in weapons:
        speeds = []
        for zooms in (0, 1, 2):
            give(w, 10, 30)
            ins = idle(90, 0, YAW)
            for _ in range(zooms):
                ins += [(IN_ATTACK2, 0, 0, 0, YAW)] + idle(40, 0, YAW)
            n0 = len(ins)
            ins += [(IN_FORWARD, 400.0, 0, 0, YAW)] * 160
            r = run("mv", ins, start=start())
            fov = r.w[max(r.w)].get("fov")
            speeds.append((zooms, round(top_speed(r, n0 + 100), 3), fov))
        print(w, speeds, flush=True)


def back():
    for w in ("weapon_knife", "weapon_ak47"):
        give(w, 10, 30)
        res = []
        for f, s in ((400.0, 0.0), (-400.0, 0.0), (0.0, 400.0), (-400.0, 400.0), (400.0, 400.0)):
            b = (IN_FORWARD if f > 0 else IN_BACK if f < 0 else 0) | (IN_MOVERIGHT if s > 0 else 0)
            ins = idle(90, 0, YAW) + [(b, f, s, 0, YAW)] * 160
            r = run("mv", ins, start=start())
            res.append(((f, s), round(top_speed(r, 190), 3)))
        print(w, res, flush=True)


def eye_trace(r, a, b):
    return [(t, round(r.ticks[t][10], 3), int(r.ticks[t][8]), round(r.ticks[t][9], 2)) for t in range(a, b) if t in r.ticks]


def duck():
    # One press held: how long the eye takes to go down; then release.
    ins = idle(60, 0, YAW) + [(IN_DUCK, 0, 0, 0, YAW)] * 60 + idle(60, 0, YAW)
    r = run("mv", ins, start=start())
    tr = eye_trace(r, 58, 180)
    changes = [x for i, x in enumerate(tr) if i == 0 or x[1:] != tr[i - 1][1:]]
    print("hold duck 60 ticks then release:", changes, flush=True)
    # Spam: press 4 ticks, release 4 ticks, 20 times; then hold.
    for on, off in ((4, 4), (2, 2), (8, 8), (14, 14)):
        ins = idle(60, 0, YAW)
        for _ in range(16):
            ins += [(IN_DUCK, 0, 0, 0, YAW)] * on + idle(off, 0, YAW)
        ins += idle(80, 0, YAW)
        r = run("mv", ins, start=start())
        tr = eye_trace(r, 58, len(ins) + 1)
        changes = [x for i, x in enumerate(tr) if i == 0 or x[1:] != tr[i - 1][1:]]
        mins = min(x[1] for x in tr)
        print(f"spam {on}/{off}: lowest eye {mins}; changes {changes[:60]}", flush=True)


if __name__ == "__main__":
    what = sys.argv[1] if len(sys.argv) > 1 else "all"
    if what in ("scoped", "all"):
        scoped()
    if what in ("back", "all"):
        back()
    if what in ("duck", "all"):
        duck()
