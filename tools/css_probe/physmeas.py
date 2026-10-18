"""Physics prop motion on the probe server (2026-10-18;
specs/cs_source/physics_props.md open questions 1, 3-8). Needs the
mashup_props and mashup_ents plugins.

    python3 tools/css_probe/physmeas.py floor x0 y0 x1 y1 step   # flat brush floors
    python3 tools/css_probe/physmeas.py rest <model> [x y z]     # rest height
    python3 tools/css_probe/physmeas.py drop <model> <height> [x y z]  # bounces
    python3 tools/css_probe/physmeas.py slide <model> <speed> [x y z]  # friction
    python3 tools/css_probe/physmeas.py drag <model> <speed>     # air drag
    python3 tools/css_probe/physmeas.py sleep <model> <height> [x y z] # sleep

Each spawns a fresh prop_physics, follows its origin every server frame
(`mashup_htrack`) and, for `sleep`, polls `physics_report_active` (the
awake objects) over RCON. Prints the track's summary; `--raw` prints it all.
"""

import json
import os
import re
import sys
import time

sys.path.insert(0, os.path.dirname(__file__))
from rcon import Rcon  # noqa: E402

DS = os.path.expanduser("~/games/css-ds/cstrike")
TRACK = "mashup/physmeas_track.txt"
# A flat tile floor on de_dust2 (z 0, found with `floor`; clear 200 units in +x).
FLAT = (320.0, 1540.0, None)

_r = None


def rcon(cmd):
    global _r
    if _r is None:
        _r = Rcon()
    return _r.exec(cmd)


def floor_at(x, y, z=200.0):
    out = rcon(f"mashup_wall {x} {y} {z} 90 0")
    m = re.search(r"entry \S+ \S+ (\S+) .*? ent (\S+) .*? props (\d+) .*?surf (\S+)", out)
    if not m:
        return None
    return float(m.group(1)), int(m.group(3)), m.group(4)


def spawn(model, x, y, z, pitch=0.0, yaw=0.0, cls="prop_physics", *kv):
    out = rcon(f"mashup_spawn {cls} {model} {x} {y} {z} {pitch} {yaw} " + " ".join(kv))
    m = re.search(r"mashup_spawn: (\d+)", out)
    if not m:
        raise RuntimeError(out)
    return int(m.group(1))


def track(e, secs, poll_active=False, name=None):
    path = os.path.join(DS, TRACK)
    rcon(f"mashup_htrack {e} {TRACK}")
    active = []
    t_end = time.time() + secs
    while time.time() < t_end:
        if poll_active:
            out = rcon("physics_report_active")
            active.append((time.time(), name in out if name else bool(out.strip())))
        time.sleep(0.02 if poll_active else 0.1)
    rcon("mashup_htrack -1")
    rows = []
    with open(path) as fh:
        for ln in fh:
            p = ln.split()
            if len(p) == 4:
                rows.append(tuple(float(v) for v in p))
    return rows, active


def summary(rows, every=1):
    for r in rows[::every]:
        print("%.4f %.3f %.3f %.3f" % r)


def peaks(rows):
    """Local maxima of z after the first contact."""
    out = []
    for i in range(1, len(rows) - 1):
        if rows[i][3] > rows[i - 1][3] and rows[i][3] >= rows[i + 1][3]:
            out.append(rows[i])
    return out


def main():
    a = sys.argv[1:]
    raw = "--raw" in a
    a = [v for v in a if v != "--raw"]
    what = a[0]
    if what == "floor":
        x0, y0, x1, y1, step = map(float, a[1:6])
        x = x0
        while x <= x1:
            y = y0
            while y <= y1:
                f = floor_at(x, y)
                if f and "displacement" not in f[2]:
                    print(x, y, f)
                y += step
            x += step
        return
    model = a[1]
    if what == "rest":
        x, y = (float(a[2]), float(a[3])) if len(a) > 3 else FLAT[:2]
        fz = floor_at(x, y)[0]
        e = spawn(model, x, y, fz + float(a[4]) if len(a) > 4 else fz + 30)
        rows, _ = track(e, 4.0)
        print("floor", fz, "rest origin", rows[-1], "above floor %.3f" % (rows[-1][3] - fz))
        print(rcon(f"mashup_einfo {e}").strip())
        rcon(f"mashup_kill {e}")
    elif what == "drop":
        h = float(a[2])
        x, y = (float(a[3]), float(a[4])) if len(a) > 4 else FLAT[:2]
        fz = floor_at(x, y)[0]
        e = spawn(model, x, y, fz + h)
        rows, _ = track(e, 5.0)
        print("floor", fz)
        if raw:
            summary(rows)
        for p in peaks(rows):
            print("peak t=%.4f z=%.3f (above rest %.3f)" % (p[0], p[3], p[3] - rows[-1][3]))
        print("rest", rows[-1])
        rcon(f"mashup_kill {e}")
    elif what == "slide":
        v = float(a[2])
        x, y = (float(a[3]), float(a[4])) if len(a) > 4 else FLAT[:2]
        dz = float(a[5]) if len(a) > 5 else 0.0
        fz = floor_at(x, y)[0]
        e = spawn(model, x, y, fz + 20)
        rows, _ = track(e, 2.5)
        rest = rows[-1]
        rcon(f"mashup_tp {e} {rest[1]} {rest[2]} {rest[3] + dz} {v} 0 0")
        rows, _ = track(e, 3.0)
        summary(rows, 1 if raw else 5)
        rcon(f"mashup_kill {e}")
    elif what == "drag":
        v = float(a[2])
        x, y, z = (float(a[3]), float(a[4]), float(a[5])) if len(a) > 5 else (0.0, 1000.0, 2000.0)
        e = spawn(model, x, y, z)
        rcon(f"mashup_tp {e} {x} {y} {z} {v} 0 {v}")
        rows, _ = track(e, 1.2)
        summary(rows, 1 if raw else 5)
        rcon(f"mashup_kill {e}")
    elif what == "sleep":
        h = float(a[2])
        x, y = (float(a[3]), float(a[4])) if len(a) > 4 else FLAT[:2]
        fz = floor_at(x, y)[0]
        e = spawn(model, x, y, fz + h)
        t0 = time.time()
        rows, active = track(e, float(a[5]) if len(a) > 5 else 8.0, poll_active=True, name=model)
        gt0 = rows[0][0]
        # wall clock to game time: the first row is the frame after the spawn
        last_active = max((t for t, on in active if on), default=t0)
        print("asleep after %.3f s of wall time" % (last_active - t0))
        # the last frame the prop moved
        moved = [r for i, r in enumerate(rows[1:], 1) if
                 max(abs(r[k] - rows[i - 1][k]) for k in (1, 2, 3)) > 1e-3]
        if moved:
            print("last move at %.3f s game time (%.4f per frame then)" % (moved[-1][0] - gt0, 0))
        print(json.dumps([(round(t - t0, 3), on) for t, on in active][::5]))
        rcon(f"mashup_kill {e}")


if __name__ == "__main__":
    main()
