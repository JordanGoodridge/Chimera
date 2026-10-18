"""Flashes with something between (specs/cs_source/grenades.md Q12,
2026-10-18, fifth pass).

    python3 tools/css_probe/flashmeas.py '{"at": [150, 2384, -120], "vpos": [250, 2384, -126.2], "yaw": 180}'

The second bot throws a flashbang at the run bot (as blastmeas.py
`flash`); while it flies, the projectile is found (`mashup_find
flashbang_projectile`) and teleported to `at` with no velocity, so it
goes off where it was put whatever the throw did. Then anything can stand
between: `blocker` [x, y, z, yaw] holds a third bot there (`mashup_targetn
1`; add one with `bot_add_t` first), `prop` spawns a motion-disabled prop
(blastmeas.py's). Prints the detonation, the run bot's first new flash
row (`F tick duration max_alpha`; `m_flFlashDuration` keeps its
last value, so the `player_blind` list says who was blinded this time) and what the fit without anything
between gives: 7.14 x (1 - d/1500) x (0.7 + 0.3 cos), d from the
detonation to the eye (64 units over the feet).

Needs mashup_props loaded and two bots (three with a blocker).
"""

import json
import math
import os
import re
import sys
import threading
import time

sys.path.insert(0, os.path.dirname(__file__))
import weapcmp as W  # noqa: E402
from blastmeas import kill, spawn_prop  # noqa: E402
from rcon import Rcon  # noqa: E402

MOVED = []


def fit(at, vpos, yaw, eye=64.0):
    e = (vpos[0], vpos[1], vpos[2] + eye)
    d = math.dist(at, e)
    f = (math.cos(math.radians(yaw)), math.sin(math.radians(yaw)), 0.0)
    to = [(a - b) / d for a, b in zip(at, e)]
    c = sum(x * y for x, y in zip(f, to))
    return d, max(0.0, 7.14 * (1 - d / 1500) * (0.7 + 0.3 * c))


def mover(at, stop):
    """Teleports the first flashbang projectile seen to `at` (its own RCON
    connection: the run's is busy)."""
    rc = Rcon()
    while not stop.is_set():
        out = rc.exec("mashup_find flashbang_projectile")
        m = re.match(r"\s*(\d+) flashbang_projectile", out)
        if m:
            rc.exec(f"mashup_tp {m.group(1)} {at[0]} {at[1]} {at[2]} 0 0 0")
            MOVED.append((m.group(1), time.time()))
            return
        time.sleep(0.03)


def flash(at, vpos, yaw=180.0, spos=None, blocker=None, prop=None, n=220, tick=20, duck=0):
    spos = spos or [vpos[0] - 150.0, vpos[1] + 150.0, vpos[2]]
    W.rcon("mp_friendlyfire 1")
    W.rcon("mashup_health 1000 0")
    W.give("weapon_knife")
    e = spawn_prop(prop) if prop else None
    if blocker:
        W.rcon(f"mashup_targetn 1 {blocker[0]} {blocker[1]} {blocker[2]} {blocker[3] if len(blocker) > 3 else 0} 1")
    W.rcon(f"mashup_shooter {spos[0]} {spos[1]} {spos[2]} weapon_flashbang {tick} 0 1")
    time.sleep(1.5)
    stop = threading.Event()
    th = threading.Thread(target=mover, args=(at, stop))
    th.start()
    buttons = 4 if duck else 0
    try:
        r = W.run("flm", [(buttons, 0.0, 0.0, 0.0, yaw)] * n, start=(*vpos, 0.0, yaw))
    finally:
        stop.set()
        th.join()
    if e is not None:
        kill(e)
    if blocker:
        W.rcon("mashup_targetn 1 off")
    g = [x for x in r.extra if x[0] == "G"]
    fl = [x for x in r.extra if x[0] == "F" and g and int(x[1]) >= int(g[0][1])]
    b = [float(x) for x in g[0][3:6]] if g else None
    if b is None:
        return "no detonation"
    d, want = fit(b, vpos, yaw, 46.0 if duck else 64.0)
    first = fl[0][2:4] if fl else ["none"]
    blind = sorted({x[2][0] for x in r.events if x[1] == "player_blind"})
    return f"B=({b[0]:.1f},{b[1]:.1f},{b[2]:.1f}) d={d:.1f} F={' '.join(first)} fit={want:.2f} blind={','.join(blind)}"


if __name__ == "__main__":
    a = json.loads(sys.argv[1])
    reps = a.pop("reps", 1)
    for _ in range(reps):
        print(flash(**a))
