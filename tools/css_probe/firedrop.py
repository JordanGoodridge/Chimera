"""Where a map's env_fires land when started (specs/source/fire.md Q5,
2026-10-18).

    python3 tools/css_probe/firedrop.py <map>...

Changes to each map, sends StartFire to every env_fire, reads where each
one went (`mashup_find env_fire`: the drop to the floor moves the
entity's origin) and puts them out again. Prints `name x y z_before ->
z_after`. Needs mashup_props loaded; leaves the server on the last map.
"""

import os
import re
import sys
import time

sys.path.insert(0, os.path.dirname(__file__))
import weapcmp as W  # noqa: E402


def fires():
    out = []
    for ln in W.rcon("mashup_find env_fire").splitlines():
        m = re.match(r"(\d+) env_fire '([^']*)'\s+at (\S+) (\S+) (\S+)", ln)
        if m:
            out.append((int(m[1]), m[2], float(m[3]), float(m[4]), float(m[5])))
    return out


def main():
    for name in sys.argv[1:]:
        W.rcon(f"changelevel {name}")
        time.sleep(15)
        before = fires()
        for e, *_ in before:
            W.rcon(f"mashup_input {e} StartFire")
        time.sleep(1.0)
        after = {e: z for e, _, _, _, z in fires()}
        for e, *_ in before:
            W.rcon(f"mashup_input {e} Extinguish")
        for e, n, x, y, z in before:
            print(f"{name} {n} {x:.2f} {y:.2f} {z:.2f} -> {after.get(e, float('nan')):.2f}", flush=True)


if __name__ == "__main__":
    main()
